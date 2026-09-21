from django.contrib.auth.models import AbstractUser
from django.db import models

from .crypto import decrypt_token, encrypt_token


class User(AbstractUser):
    """Strava-linked player account; regular players have no local password."""

    athlete_id = models.BigIntegerField(unique=True, null=True, blank=True)

    strava_access_token_encrypted = models.TextField(blank=True, default="")
    strava_refresh_token_encrypted = models.TextField(blank=True, default="")
    strava_token_expires_at = models.DateTimeField(null=True, blank=True)

    @property
    def strava_access_token(self) -> str:
        """Decrypted access token, or "" if unset."""
        return decrypt_token(self.strava_access_token_encrypted)

    @strava_access_token.setter
    def strava_access_token(self, value: str) -> None:
        """Encrypt and store the access token."""
        self.strava_access_token_encrypted = encrypt_token(value)

    @property
    def strava_refresh_token(self) -> str:
        """Decrypted refresh token, or "" if unset."""
        return decrypt_token(self.strava_refresh_token_encrypted)

    @strava_refresh_token.setter
    def strava_refresh_token(self, value: str) -> None:
        """Encrypt and store the refresh token."""
        self.strava_refresh_token_encrypted = encrypt_token(value)

    def __str__(self) -> str:
        """Admin/debug label: username, or "athlete:<id>" if unset."""
        return self.username or f"athlete:{self.athlete_id}"
