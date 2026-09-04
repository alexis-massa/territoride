from datetime import UTC, datetime
from typing import Any

import gpxpy
from django.contrib.gis.geos import LineString
from django.core.management.base import BaseCommand, CommandError, CommandParser

from accounts.models import User
from game.capture import capture_territory
from game.models import Activity


class Command(BaseCommand):
    help = "Load a GPX file as an Activity for a player and capture its territory"

    def add_arguments(self, parser: CommandParser) -> None:
        """Declare the command's positional arguments."""
        parser.add_argument("gpx_path")
        parser.add_argument("username")

    def handle(self, *args: Any, **options: Any) -> None:
        """Parse the GPX file, store it as an Activity, and capture its territory.

        Args:
            *args: Unused positional arguments from Django's command framework.
            **options: Parsed command options; "gpx_path" and "username" are used.
        """
        try:
            user = User.objects.get(username=options["username"])
        except User.DoesNotExist as exc:
            raise CommandError(f"No user named {options['username']!r}") from exc

        with open(options["gpx_path"]) as gpx_file:
            gpx = gpxpy.parse(gpx_file)

        points = [p for track in gpx.tracks for segment in track.segments for p in segment.points]
        if len(points) < 2:
            raise CommandError("GPX file has fewer than 2 track points")

        activity = Activity.objects.create(
            user=user,
            name=(gpx.tracks[0].name or options["gpx_path"]),
            track=LineString([(p.longitude, p.latitude) for p in points], srid=4326),
            recorded_at=points[0].time or datetime.now(UTC),
        )
        captured = capture_territory(activity)
        self.stdout.write(f"Loaded {activity.name!r}: captured {captured} cells")
