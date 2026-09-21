from typing import Any

from django.core.management.base import BaseCommand

from game.capture import recapture_all
from game.models import Activity


class Command(BaseCommand):
    help = "Replay capture for every activity, oldest first - needed after POIs are added"

    def handle(self, *args: Any, **options: Any) -> None:
        """Recompute territory and POI captures for every activity."""
        cells, pois = recapture_all()
        self.stdout.write(
            f"Recaptured {cells} cells and {pois} POIs across {Activity.objects.count()} activities"
        )
