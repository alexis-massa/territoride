from django.contrib.auth.models import AbstractUser
from django.db import models

from .crypto import EncryptedTextField


class User(AbstractUser):
    """Strava-linked player account; regular players have no local password."""

    athlete_id = models.BigIntegerField(unique=True, null=True, blank=True)

    strava_access_token_encrypted = models.TextField(blank=True, default="")
    strava_refresh_token_encrypted = models.TextField(blank=True, default="")
    strava_token_expires_at = models.DateTimeField(null=True, blank=True)

    strava_access_token = EncryptedTextField("strava_access_token_encrypted")
    strava_refresh_token = EncryptedTextField("strava_refresh_token_encrypted")

    def __str__(self) -> str:
        """One-line label for admin/debug output.

        Returns:
            The username, or "athlete:<id>" if unset.
        """
        return self.username or f"athlete:{self.athlete_id}"
