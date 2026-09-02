from django.contrib.auth.models import AbstractUser
from django.db import models

from .crypto import decrypt_token, encrypt_token


class User(AbstractUser):
    """Strava-first account. Regular players never set a local password —
    only an admin-created superuser (for Django admin access) has one."""

    athlete_id = models.BigIntegerField(unique=True, null=True, blank=True)
    is_allowed = models.BooleanField(
        default=False,
        help_text="Gates access for this friends-only game. Flip on to let the player in.",
    )

    strava_access_token_encrypted = models.TextField(blank=True, default="")
    strava_refresh_token_encrypted = models.TextField(blank=True, default="")
    strava_token_expires_at = models.DateTimeField(null=True, blank=True)

    @property
    def strava_access_token(self) -> str:
        return (
            decrypt_token(self.strava_access_token_encrypted)
            if self.strava_access_token_encrypted
            else ""
        )

    @strava_access_token.setter
    def strava_access_token(self, value: str) -> None:
        self.strava_access_token_encrypted = encrypt_token(value)

    @property
    def strava_refresh_token(self) -> str:
        return (
            decrypt_token(self.strava_refresh_token_encrypted)
            if self.strava_refresh_token_encrypted
            else ""
        )

    @strava_refresh_token.setter
    def strava_refresh_token(self, value: str) -> None:
        self.strava_refresh_token_encrypted = encrypt_token(value)

    def __str__(self) -> str:
        return self.username or f"athlete:{self.athlete_id}"
