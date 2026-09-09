import contextlib
import json
import secrets
from datetime import UTC, datetime, timedelta
from typing import Any, TypedDict
from urllib.parse import urlencode

import requests
from django.conf import settings
from django.contrib import messages
from django.contrib.auth import login, logout
from django.contrib.auth.decorators import login_required
from django.http import (
    HttpRequest,
    HttpResponse,
    HttpResponseBadRequest,
    HttpResponseForbidden,
    HttpResponseRedirect,
    JsonResponse,
)
from django.shortcuts import redirect, render
from django.urls import reverse
from django.views.decorators.csrf import csrf_exempt
from django.views.decorators.http import require_POST

from game.capture import recapture_all, release_activity
from game.decay import DECAY_THRESHOLD_DAYS
from game.models import POI, Activity
from game.scoring import leaderboard, player_color, player_score, score_split_pct
from game.strava_import import import_from_strava

from .models import User

STRAVA_AUTHORIZE_URL = "https://www.strava.com/oauth/authorize"
STRAVA_TOKEN_URL = "https://www.strava.com/oauth/token"
STRAVA_ACTIVITIES_URL = "https://www.strava.com/api/v3/athlete/activities"
STRAVA_ACTIVITY_DETAIL_URL = "https://www.strava.com/api/v3/activities"
STRAVA_DEAUTHORIZE_URL = "https://www.strava.com/oauth/deauthorize"
STRAVA_PAGE_SIZE = 30

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


def _ensure_valid_token(user: User) -> str:
    """Refresh the player's stored Strava access token if it has expired.

    Args:
        user: The player whose Strava connection to check.

    Returns:
        A valid access token.
    """
    if user.strava_token_expires_at and user.strava_token_expires_at > datetime.now(UTC):
        return user.strava_access_token

    response = requests.post(
        STRAVA_TOKEN_URL,
        data={
            "client_id": settings.STRAVA_CLIENT_ID,
            "client_secret": settings.STRAVA_CLIENT_SECRET,
            "refresh_token": user.strava_refresh_token,
            "grant_type": "refresh_token",
        },
        timeout=10,
    )
    response.raise_for_status()
    payload = response.json()
    user.strava_access_token = payload["access_token"]
    user.strava_refresh_token = payload["refresh_token"]
    user.strava_token_expires_at = datetime.fromtimestamp(payload["expires_at"], tz=UTC)
    user.save()
    return user.strava_access_token


def _fetch_recent_strava_activities(token: str) -> list[dict[str, Any]]:
    """Fetch every Strava activity within the decay window.

    Activities older than that have no value even if imported, since they'd
    already be past DECAY_THRESHOLD_DAYS.

    Args:
        token: A valid Strava access token.

    Returns:
        Raw activity summaries from Strava, newest first.
    """
    cutoff = datetime.now(UTC) - timedelta(days=DECAY_THRESHOLD_DAYS)
    activities: list[dict[str, Any]] = []
    page = 1
    while True:
        response = requests.get(
            STRAVA_ACTIVITIES_URL,
            headers={"Authorization": f"Bearer {token}"},
            params={"after": int(cutoff.timestamp()), "per_page": STRAVA_PAGE_SIZE, "page": page},
            timeout=10,
        )
        response.raise_for_status()
        batch = response.json()
        activities.extend(batch)
        if len(batch) < STRAVA_PAGE_SIZE:
            return activities
        page += 1


@login_required
@require_POST
def sync_strava_view(request: HttpRequest) -> HttpResponse:
    """Pull the player's recent Strava activities and capture territory/POIs.

    Args:
        request: The incoming request.

    Returns:
        A redirect to the map, with a status message.
    """
    assert isinstance(request.user, User)
    if not request.user.strava_refresh_token:
        messages.error(request, "Connect with Strava first.")
        return redirect("map")

    try:
        token = _ensure_valid_token(request.user)
        strava_activities = _fetch_recent_strava_activities(token)
    except requests.HTTPError as exc:
        detail = exc.response.text[:200] if exc.response is not None else str(exc)
        messages.error(request, f"Strava returned an error: {detail}")
        return redirect("map")
    except requests.RequestException:
        messages.error(request, "Couldn't reach Strava.")
        return redirect("map")

    imported, cells, pois = import_from_strava(request.user, strava_activities)
    if imported:
        messages.success(
            request, f"Synced {imported} activities: captured {cells} cells, {pois} passes."
        )
    else:
        messages.success(request, "No new activities to sync.")
    return redirect("map")


def _handle_strava_event(event: dict[str, Any]) -> None:
    """Process one Strava webhook event: a new/updated activity, or deauthorization.

    Args:
        event: The decoded webhook payload.
    """
    if event.get("object_type") != "activity":
        return
    try:
        user = User.objects.get(athlete_id=event.get("owner_id"))
    except User.DoesNotExist:
        return

    if event.get("updates", {}).get("authorized") == "false":
        user.strava_access_token = ""
        user.strava_refresh_token = ""
        user.strava_token_expires_at = None
        user.save()
        return

    if event.get("aspect_type") == "delete":
        activity = Activity.objects.filter(strava_activity_id=event["object_id"]).first()
        if activity is not None:
            release_activity(activity)
        return

    if event.get("aspect_type") not in ("create", "update"):
        return

    token = _ensure_valid_token(user)
    response = requests.get(
        f"{STRAVA_ACTIVITY_DETAIL_URL}/{event['object_id']}",
        headers={"Authorization": f"Bearer {token}"},
        timeout=10,
    )
    response.raise_for_status()
    import_from_strava(user, [response.json()])


@csrf_exempt
def strava_webhook_view(request: HttpRequest) -> HttpResponse:
    """Handle Strava's webhook subscription handshake and activity events.

    Args:
        request: The incoming request; GET is the one-time subscription
            challenge, POST delivers activity/deauthorization events.

    Returns:
        The echoed challenge on GET; an empty 200 on POST, since Strava
        requires a fast response regardless of processing outcome.
    """
    if request.method == "GET":
        if request.GET.get("hub.verify_token") != settings.STRAVA_WEBHOOK_VERIFY_TOKEN:
            return HttpResponseForbidden()
        return JsonResponse({"hub.challenge": request.GET.get("hub.challenge", "")})

    _handle_strava_event(json.loads(request.body))
    return HttpResponse()


@login_required
@require_POST
def delete_account_view(request: HttpRequest) -> HttpResponse:
    """Delete the player's account: revoke Strava access, wipe their data,
    and let any surviving activity reclaim what they held.

    Args:
        request: The incoming request.

    Returns:
        A redirect to the map, logged out, with a status message.
    """
    assert isinstance(request.user, User)
    user = request.user

    if user.strava_access_token:
        with contextlib.suppress(requests.RequestException):
            requests.post(
                STRAVA_DEAUTHORIZE_URL,
                data={"access_token": user.strava_access_token},
                timeout=10,
            )

    username = user.username
    user.delete()
    recapture_all()
    logout(request)

    messages.success(request, f"Account {username!r} deleted.")
    return redirect("map")


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
