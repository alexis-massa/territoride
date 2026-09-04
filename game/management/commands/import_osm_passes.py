import time
from typing import Any

import requests
from django.contrib.gis.geos import Point
from django.core.management.base import BaseCommand

from game.models import POI

OVERPASS_URL = "https://overpass-api.de/api/interpreter"

# Rough bounding box for the former Rhone-Alpes region: (south, west, north, east)
RHONE_ALPES_BBOX = (44.0, 4.0, 46.5, 7.3)

# The public Overpass instance is a shared, sometimes-overloaded community
# service - retry a couple of times before giving up. It also rejects
# requests with no identifying User-Agent (its usage policy asks for one).
MAX_ATTEMPTS = 3
RETRY_DELAY_SECONDS = 5
REQUEST_HEADERS = {"User-Agent": "TerritoRide/0.1 (hobby project; not for production use)"}


def _parse_altitude(ele: str | None) -> int | None:
    """Parse OSM's ele tag into whole meters.

    Args:
        ele: The raw "ele" tag value, if present.

    Returns:
        The altitude in meters, or None if missing/unparseable.
    """
    if ele is None:
        return None
    try:
        return round(float(ele))
    except ValueError:
        return None


class Command(BaseCommand):
    help = "Import named mountain passes from OpenStreetMap within Rhone-Alpes"

    def handle(self, *args: Any, **options: Any) -> None:
        """Fetch mountain_pass=yes nodes from Overpass and create any new POIs.

        Args:
            *args: Unused positional arguments from Django's command framework.
            **options: Unused parsed options from Django's command framework.
        """
        south, west, north, east = RHONE_ALPES_BBOX
        query = (
            "[out:json][timeout:25];"
            f'node["mountain_pass"="yes"]["name"]({south},{west},{north},{east});'
            "out body;"
        )
        elements = self._fetch_elements(query)

        created = 0
        for element in elements:
            _, was_created = POI.objects.get_or_create(
                name=element["tags"]["name"],
                defaults={
                    "location": Point(element["lon"], element["lat"], srid=4326),
                    "altitude_m": _parse_altitude(element["tags"].get("ele")),
                },
            )
            if was_created:
                created += 1

        skipped = len(elements) - created
        self.stdout.write(
            f"Created {created} new POIs from OpenStreetMap ({skipped} already existed)"
        )

    def _fetch_elements(self, query: str) -> list[dict[str, Any]]:
        """POST a query to Overpass, retrying on transient server errors.

        Args:
            query: The Overpass QL query string.

        Returns:
            The "elements" list from Overpass's JSON response.
        """
        for attempt in range(1, MAX_ATTEMPTS + 1):
            response = requests.post(
                OVERPASS_URL, data={"data": query}, headers=REQUEST_HEADERS, timeout=30
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
