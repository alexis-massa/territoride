from typing import Any

import requests
from django.core.management.base import BaseCommand

from accounts.models import User
from accounts.views import _ensure_valid_token, _fetch_recent_strava_activities
from game.strava_import import import_from_strava


class Command(BaseCommand):
    help = "Sync recent Strava activities for every connected player, not just the current user"

    def handle(self, *args: Any, **options: Any) -> None:
        """Run the same sync as the "Sync from Strava" button, for every connected player.

        Args:
            *args: Unused positional arguments from Django's command framework.
            **options: Unused parsed options from Django's command framework.
        """
        players = User.objects.exclude(strava_refresh_token_encrypted="")
        for player in players:
            try:
                token = _ensure_valid_token(player)
                strava_activities = _fetch_recent_strava_activities(token)
            except requests.RequestException as exc:
                self.stdout.write(f"{player}: skipped, Strava error ({exc})")
                continue

            imported, cells, pois = import_from_strava(player, strava_activities)
            self.stdout.write(
                f"{player}: imported {imported}, captured {cells} cells, {pois} passes"
            )
