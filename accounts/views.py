import secrets
from datetime import UTC, datetime
from urllib.parse import urlencode

import requests
from django.conf import settings
from django.contrib.auth import login
from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, HttpResponseRedirect
from django.shortcuts import render
from django.urls import reverse

from .models import User

STRAVA_AUTHORIZE_URL = "https://www.strava.com/oauth/authorize"
STRAVA_TOKEN_URL = "https://www.strava.com/oauth/token"


def strava_authorize(request: HttpRequest) -> HttpResponse:
    """Start the Strava OAuth handshake.

    Args:
        request: The incoming request; used for the session and callback URL.

    Returns:
        A redirect to Strava's OAuth authorize page.
    """
    state = secrets.token_urlsafe(32)
    request.session["strava_oauth_state"] = state

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
    error = request.GET.get("error")
    if error:
        return _popup_response(request, status="error", message=f"Strava denied access: {error}")

    state = request.GET.get("state")
    expected_state = request.session.pop("strava_oauth_state", None)
    if not state or state != expected_state:
        return HttpResponseBadRequest("Invalid OAuth state")

    code = request.GET.get("code")
    if not code:
        return _popup_response(request, status="error", message="Missing authorization code")

    token_response = requests.post(
        STRAVA_TOKEN_URL,
        data={
            "client_id": settings.STRAVA_CLIENT_ID,
            "client_secret": settings.STRAVA_CLIENT_SECRET,
            "code": code,
            "grant_type": "authorization_code",
        },
        timeout=10,
    )
    if not token_response.ok:
        return _popup_response(request, status="error", message="Could not reach Strava")

    payload = token_response.json()
    athlete = payload["athlete"]

    user, _created = User.objects.get_or_create(
        athlete_id=athlete["id"],
        defaults={"username": athlete.get("username") or f"athlete_{athlete['id']}"},
    )
    user.first_name = athlete.get("firstname") or ""
    user.last_name = athlete.get("lastname") or ""
    if not user.has_usable_password():
        user.set_unusable_password()
    user.strava_access_token = payload["access_token"]
    user.strava_refresh_token = payload["refresh_token"]
    user.strava_token_expires_at = datetime.fromtimestamp(payload["expires_at"], tz=UTC)
    user.save()

    login(request, user, backend="django.contrib.auth.backends.ModelBackend")
    return _popup_response(request, status="success", message="Connected!")


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
