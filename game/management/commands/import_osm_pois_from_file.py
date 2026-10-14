import json
from pathlib import Path
from typing import Any

from django.contrib.gis.geos import Point
from django.core.management.base import BaseCommand, CommandParser

from game.models import POI


class Command(BaseCommand):
    help = "Load POIs from an NDJSON file produced by scripts/extract_osm_pois.py"

    def add_arguments(self, parser: CommandParser) -> None:
        """Declare the command's arguments."""
        parser.add_argument("path", type=Path)

    def handle(self, *args: Any, **options: Any) -> None:
        """Stream NDJSON records into POI rows, reporting progress as it goes."""
        path: Path = options["path"]
        fetched = created = 0
        with path.open() as lines:
            for line in lines:
                line = line.strip()
                if not line:
                    continue
                record = json.loads(line)
                _, was_created = POI.objects.get_or_create(
                    name=record["name"],
                    poi_type=record["poi_type"],
                    defaults={
                        "location": Point(record["lon"], record["lat"], srid=4326),
                        "altitude_m": record["altitude_m"],
                    },
                )
                fetched += 1
                if was_created:
                    created += 1
                if fetched % 1000 == 0:
                    self.stdout.write(f"  {fetched} processed ({created} new so far)")
        skipped = fetched - created
        self.stdout.write(
            self.style.SUCCESS(
                f"Done: {created} new POIs ({skipped} already existed, {fetched} total)"
            )
        )
