import time
from typing import Any

import requests
from django.contrib.gis.geos import Point
from django.core.management.base import BaseCommand, CommandParser

from game.models import POI

OVERPASS_URL = "https://overpass-api.de/api/interpreter"

# The public Overpass instance is a shared, sometimes-overloaded community
# service - retry a couple of times before giving up. It also rejects
# requests with no identifying User-Agent (its usage policy asks for one).
MAX_ATTEMPTS = 3
RETRY_DELAY_SECONDS = 5
REQUEST_HEADERS = {"User-Agent": "TerritoRide/0.1 (hobby project; not for production use)"}

# Summits worldwide number in the millions - named ones alone are still huge,
# so also require elevation data (no altitude, no points - not worth
# importing) and some sign of significance: a notably high elevation, a
# Wikipedia article, or a tagged topographic prominence. Any one is enough.
SUMMIT_MIN_ELEVATION_M = 1000

QUERIES = {
    POI.POIType.MOUNTAIN_PASS: (
        '[out:json][timeout:90];node["mountain_pass"="yes"]["name"];out body;'
    ),
    POI.POIType.SUMMIT: (
        "[out:json][timeout:180];"
        'node["natural"="peak"]["name"]'
        '(if: t["ele"] != "" && ('
        f'number(t["ele"]) > {SUMMIT_MIN_ELEVATION_M}'
        ' || t["wikipedia"] != "" || t["prominence"] != ""));'
        "out body;"
    ),
}


def _parse_altitude(ele: str | None) -> int | None:
    """Parse OSM's "ele" tag into whole meters, or None if missing/unparseable."""
    if ele is None:
        return None
    try:
        return round(float(ele))
    except ValueError:
        return None


class Command(BaseCommand):
    help = "Import named POIs (mountain passes or summits) from OpenStreetMap, worldwide"

    def add_arguments(self, parser: CommandParser) -> None:
        """Declare the command's arguments."""
        parser.add_argument("poi_type", choices=POI.POIType.values)

    def handle(self, *args: Any, **options: Any) -> None:
        """Fetch the requested POI type from Overpass and create any new POIs."""
        poi_type = options["poi_type"]
        label = POI.POIType(poi_type).label
        self.stdout.write(f"Querying Overpass for {label} POIs (worldwide - can take a while)...")
        elements = self._fetch_elements(QUERIES[poi_type])
        total = len(elements)
        self.stdout.write(f"Fetched {total} {label} POIs, importing...")

        created = 0
        for i, element in enumerate(elements, start=1):
            _, was_created = POI.objects.get_or_create(
                name=element["tags"]["name"],
                poi_type=poi_type,
                defaults={
                    "location": Point(element["lon"], element["lat"], srid=4326),
                    "altitude_m": _parse_altitude(element["tags"].get("ele")),
                },
            )
            if was_created:
                created += 1
            if i % 1000 == 0 or i == total:
                self.stdout.write(f"  {i}/{total} processed ({created} new so far)")

        skipped = total - created
        self.stdout.write(
            f"Created {created} new POIs from OpenStreetMap ({skipped} already existed)"
        )

    def _fetch_elements(self, query: str) -> list[dict[str, Any]]:
        """POST a query to Overpass, retrying on transient server errors."""
        for attempt in range(1, MAX_ATTEMPTS + 1):
            response = requests.post(
                OVERPASS_URL, data={"data": query}, headers=REQUEST_HEADERS, timeout=200
            )
            if response.ok:
                elements: list[dict[str, Any]] = response.json()["elements"]
                return elements
            if attempt == MAX_ATTEMPTS or response.status_code < 500:
                response.raise_for_status()
            self.stdout.write(
                f"Overpass returned {response.status_code}, retrying ({attempt}/{MAX_ATTEMPTS})..."
            )
            time.sleep(RETRY_DELAY_SECONDS)
        return []
