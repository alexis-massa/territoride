import time
from typing import Any

import requests
from django.contrib.gis.geos import Point
from django.core.management.base import BaseCommand, CommandParser

from game.models import POI

OVERPASS_URL = "https://overpass-api.de/api/interpreter"

# Overpass's shared public instance; rejects requests with no User-Agent.
MAX_ATTEMPTS = 3
RETRY_DELAY_SECONDS = 5
REQUEST_HEADERS = {"User-Agent": "TerritoRide/0.1 (hobby project; not for production use)"}
QUERY_TIMEOUT_SECONDS = 90
HTTP_TIMEOUT_SECONDS = 120

# A region that times out gets quartered and retried piece by piece.
MAX_SPLIT_DEPTH = 6
# Hard cap on total requests, in case a bug or a pathological region keeps splitting.
MAX_TOTAL_REQUESTS = 400

# Summits need a real elevation (unscoreable without one) plus some sign of
# significance - a high elevation, a Wikipedia article, or tagged prominence.
SUMMIT_MIN_ELEVATION_M = 1000

WORLD_BBOX = (-180.0, -90.0, 180.0, 90.0)  # (west, south, east, north)

# Node-matching statement only - _fetch_region adds the header/footer.
QUERY_STATEMENTS = {
    POI.POIType.MOUNTAIN_PASS: 'node["mountain_pass"="yes"]["name"];',
    POI.POIType.SUMMIT: (
        'node["natural"="peak"]["name"]'
        '(if: t["ele"] != "" && ('
        f'number(t["ele"]) > {SUMMIT_MIN_ELEVATION_M}'
        ' || t["wikipedia"] != "" || t["prominence"] != ""));'
    ),
}


class _QueryTimedOut(Exception):
    """Overpass accepted the request (HTTP 200) but its own runtime timed out."""


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
        self._request_count = 0
        elements = self._fetch_region(QUERY_STATEMENTS[poi_type], WORLD_BBOX, depth=0)
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

    def _fetch_region(
        self, statement: str, bbox: tuple[float, float, float, float], depth: int
    ) -> list[dict[str, Any]]:
        """Fetch elements for a bbox, recursively quartering it if Overpass times out."""
        if self._request_count >= MAX_TOTAL_REQUESTS:
            self.stdout.write(f"  {bbox} skipped, hit the {MAX_TOTAL_REQUESTS}-request budget")
            return []

        west, south, east, north = bbox
        query = (
            f"[out:json][timeout:{QUERY_TIMEOUT_SECONDS}][bbox:{south},{west},{north},{east}];"
            f"{statement}out body;"
        )
        try:
            return self._fetch_elements(query)
        except _QueryTimedOut:
            if depth >= MAX_SPLIT_DEPTH:
                self.stdout.write(f"  {bbox} still times out at max split depth, skipping")
                return []
            self.stdout.write(f"  {bbox} timed out, splitting into quadrants...")
            mid_lon, mid_lat = (west + east) / 2, (south + north) / 2
            quadrants = [
                (west, south, mid_lon, mid_lat),
                (mid_lon, south, east, mid_lat),
                (west, mid_lat, mid_lon, north),
                (mid_lon, mid_lat, east, north),
            ]
            elements: list[dict[str, Any]] = []
            for quadrant in quadrants:
                elements.extend(self._fetch_region(statement, quadrant, depth + 1))
            return elements

    def _fetch_elements(self, query: str) -> list[dict[str, Any]]:
        """POST to Overpass, retrying on server errors; raises _QueryTimedOut on a remark."""
        for attempt in range(1, MAX_ATTEMPTS + 1):
            self._request_count += 1
            response = requests.post(
                OVERPASS_URL, data={"data": query}, headers=REQUEST_HEADERS,
                timeout=HTTP_TIMEOUT_SECONDS,
            )
            if response.ok:
                payload = response.json()
                if "remark" in payload:
                    raise _QueryTimedOut(payload["remark"])
                elements: list[dict[str, Any]] = payload["elements"]
                return elements
            if attempt == MAX_ATTEMPTS or response.status_code < 500:
                response.raise_for_status()
            self.stdout.write(
                f"Overpass returned {response.status_code}, retrying ({attempt}/{MAX_ATTEMPTS})..."
            )
            time.sleep(RETRY_DELAY_SECONDS)
        return []
