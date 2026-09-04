import secrets
from datetime import UTC, datetime
from typing import TypedDict
from urllib.parse import urlencode

import requests
from django.conf import settings
from django.contrib.auth import login
from django.contrib.auth.decorators import login_required
from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, HttpResponseRedirect
from django.shortcuts import render
from django.urls import reverse

from game.models import POI
from game.scoring import leaderboard, player_color, player_score, score_split_pct

from .models import User

STRAVA_AUTHORIZE_URL = "https://www.strava.com/oauth/authorize"
STRAVA_TOKEN_URL = "https://www.strava.com/oauth/token"

OAUTH_STATE_SESSION_KEY = "strava_oauth_state"
ERROR_PARAM = "error"
STATE_PARAM = "state"
CODE_PARAM = "code"


class StravaAthlete(TypedDict):
    """Shape of the athlete summary Strava embeds in a token response."""

    id: int
    username: str | None
    firstname: str | None
    lastname: str | None


class StravaTokenResponse(TypedDict):
    """Shape of Strava's POST /oauth/token response."""

    access_token: str
    refresh_token: str
    expires_at: int
    athlete: StravaAthlete


def strava_authorize(request: HttpRequest) -> HttpResponse:
    """Start the Strava OAuth handshake.

    Args:
        request: The incoming request; used for the session and callback URL.

    Returns:
        A redirect to Strava's OAuth authorize page.
    """
    state = secrets.token_urlsafe(32)
    request.session[OAUTH_STATE_SESSION_KEY] = state

    redirect_uri = request.build_absolute_uri(reverse("accounts:strava_callback"))
    params = {
        "client_id": settings.STRAVA_CLIENT_ID,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "approval_prompt": "auto",
        "scope": "read,activity:read",
        "state": state,
    }
    return HttpResponseRedirect(f"{STRAVA_AUTHORIZE_URL}?{urlencode(params)}")


def strava_callback(request: HttpRequest) -> HttpResponse:
    """Handle Strava's OAuth redirect: exchange the code and log the player in.

    Args:
        request: The callback request, carrying `code` and `state` query params.

    Returns:
        The popup-closing response, reporting success or failure to the opener.
    """
    if ERROR_PARAM in request.GET:
        message = f"Strava denied access: {request.GET[ERROR_PARAM]}"
        return _popup_response(request, status="error", message=message)

    expected_state = request.session.pop(OAUTH_STATE_SESSION_KEY, None)
    if STATE_PARAM not in request.GET or request.GET[STATE_PARAM] != expected_state:
        return HttpResponseBadRequest("Invalid OAuth state")

    if CODE_PARAM not in request.GET:
        return _popup_response(request, status="error", message="Missing authorization code")

    token_response = requests.post(
        STRAVA_TOKEN_URL,
        data={
            "client_id": settings.STRAVA_CLIENT_ID,
            "client_secret": settings.STRAVA_CLIENT_SECRET,
            "code": request.GET[CODE_PARAM],
            "grant_type": "authorization_code",
        },
        timeout=10,
    )
    if not token_response.ok:
        return _popup_response(request, status="error", message="Could not reach Strava")

    payload: StravaTokenResponse = token_response.json()
    athlete = payload["athlete"]

    user, _created = User.objects.get_or_create(
        athlete_id=athlete["id"],
        defaults={"username": athlete["username"] or f"athlete_{athlete['id']}"},
    )
    user.first_name = athlete["firstname"] or ""
    user.last_name = athlete["lastname"] or ""
    if not user.has_usable_password():
        user.set_unusable_password()
    user.strava_access_token = payload["access_token"]
    user.strava_refresh_token = payload["refresh_token"]
    user.strava_token_expires_at = datetime.fromtimestamp(payload["expires_at"], tz=UTC)
    user.save()

    login(request, user, backend="django.contrib.auth.backends.ModelBackend")
    return _popup_response(request, status="success", message="Connected!")


@login_required
def profile_view(request: HttpRequest) -> HttpResponse:
    """Render the current player's profile page.

    Args:
        request: The incoming request; must be authenticated.

    Returns:
        The rendered profile page.
    """
    assert isinstance(request.user, User)
    board = leaderboard()
    rank = next(
        (i + 1 for i, row in enumerate(board) if row["username"] == request.user.username), None
    )
    score = player_score(request.user)
    territory_pct, poi_pct = score_split_pct(score)
    context = {
        "score": score,
        "territory_pct": territory_pct,
        "poi_pct": poi_pct,
        "player_color": player_color(request.user.username),
        "rank": rank,
        "player_count": len(board),
        "claimed_pois": POI.objects.filter(owner=request.user).order_by("-altitude_m"),
    }
    return render(request, "accounts/profile.html", context)


def _popup_response(request: HttpRequest, *, status: str, message: str) -> HttpResponse:
    """Render the page the OAuth popup closes itself from.

    Args:
        request: The current request.
        status: "success" or "error"; read by the opener window's JS.
        message: Text shown in the popup and relayed to the opener.

    Returns:
        The rendered oauth_complete.html response.
    """
    return render(request, "accounts/oauth_complete.html", {"status": status, "message": message})
