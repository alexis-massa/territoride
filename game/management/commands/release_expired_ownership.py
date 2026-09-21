from typing import Any

from django.core.management.base import BaseCommand

from game.decay import release_expired


class Command(BaseCommand):
    help = "Return territory cells and POIs past the decay threshold to unowned"

    def handle(self, *args: Any, **options: Any) -> None:
        """Run the expiry sweep and report how much was released."""
        released = release_expired()
        self.stdout.write(f"Released {released} expired cells/POIs")
