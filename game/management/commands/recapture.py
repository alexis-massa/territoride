from typing import Any

from django.core.management.base import BaseCommand

from game.capture import capture_pois, capture_territory
from game.models import Activity


class Command(BaseCommand):
    help = "Replay capture for every activity, oldest first - needed after POIs are added"

    def handle(self, *args: Any, **options: Any) -> None:
        """Recompute territory and POI captures for every activity.

        Args:
            *args: Unused positional arguments from Django's command framework.
            **options: Unused parsed options from Django's command framework.
        """
        activities = list(Activity.objects.order_by("recorded_at"))
        cells = pois = 0
        for activity in activities:
            cells += capture_territory(activity)
            pois += capture_pois(activity)
        self.stdout.write(
            f"Recaptured {cells} cells and {pois} POIs across {len(activities)} activities"
        )
