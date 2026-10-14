import json
import random
import time
from collections import deque
from pathlib import Path
from typing import Any

import requests
from django.contrib.gis.geos import Point
from django.core.management.base import BaseCommand, CommandParser

from game.models import POI

OVERPASS_URL = "https://overpass-api.de/api/interpreter"

# Rejects requests with no User-Agent.
MAX_ATTEMPTS = 3
# Only escalates on 429 - a 5xx/timeout on an oversized bbox is instant and unrelated to pace.
BASE_PACE_SECONDS = 2.0
MAX_PACE_SECONDS = 60.0
REQUEST_HEADERS = {"User-Agent": "TerritoRide/0.1 (hobby project; not for production use)"}
QUERY_TIMEOUT_SECONDS = 25
HTTP_TIMEOUT_SECONDS = 40

MAX_SPLIT_DEPTH = 20
MAX_TOTAL_REQUESTS = 100000

SUMMIT_MIN_ELEVATION_M = 1000

WORLD_BBOX = (-180.0, -90.0, 180.0, 90.0)  # (west, south, east, north)

# Node-matching statement only - _run_queue adds the header/footer.
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


def _retry_after_seconds(response: requests.Response) -> float | None:
    """Server-requested retry delay from a Retry-After header, if present and numeric."""
    try:
        return float(response.headers["Retry-After"])
    except (KeyError, ValueError):
        return None


def _format_bbox(bbox: tuple[float, float, float, float]) -> str:
    """Render a bbox as readable coordinates instead of a raw float tuple."""
    west, south, east, north = bbox
    return f"({west:.1f},{south:.1f})-({east:.1f},{north:.1f})"


class Command(BaseCommand):
    help = "Import named POIs (mountain passes or summits) from OpenStreetMap, worldwide"

    def add_arguments(self, parser: CommandParser) -> None:
        """Declare the command's arguments."""
        parser.add_argument("poi_type", choices=POI.POIType.values)

    def handle(self, *args: Any, **options: Any) -> None:
        """Fetch the requested POI type from Overpass, importing each batch as it arrives."""
        self._poi_type = options["poi_type"]
        label = POI.POIType(self._poi_type).label
        self.stdout.write(self.style.MIGRATE_HEADING(f"Importing {label} POIs worldwide"))
        self._request_count = 0
        self._pace_seconds = BASE_PACE_SECONDS
        self._depth = 0
        self._fetched = 0
        self._created = 0

        self._stuck: list[tuple[float, float, float, float, int]] = []
        pending_path = self._pending_path()
        if pending_path.exists():
            queue = deque(tuple(entry) for entry in json.loads(pending_path.read_text()))
            self.stdout.write(f"Resuming {len(queue)} region(s) left over from a previous run")
        else:
            queue = deque([(*WORLD_BBOX, 0)])

        try:
            self._run_queue(QUERY_STATEMENTS[self._poi_type], queue, pending_path)
        except KeyboardInterrupt:
            self.stdout.write(self.style.WARNING("\nStopped - everything fetched so far is saved."))

        remaining = list(queue) + self._stuck
        if remaining:
            pending_path.write_text(json.dumps(remaining))
            self.stdout.write(
                self.style.WARNING(
                    f"{len(remaining)} region(s) still pending in {pending_path.name} - "
                    "re-run this command to pick up where it left off."
                )
            )
        else:
            pending_path.unlink(missing_ok=True)
        skipped = self._fetched - self._created
        self.stdout.write(
            self.style.SUCCESS(
                f"Done: {self._created} new POIs ({skipped} already existed, "
                f"{self._fetched} fetched total)"
            )
        )

    def _pending_path(self) -> Path:
        return Path(f".osm_import_pending_{self._poi_type}.json")

    def _indent(self) -> str:
        return "  " * self._depth

    def _import_elements(self, elements: list[dict[str, Any]]) -> None:
        """Create any new POIs from a fetched batch immediately, so nothing is lost if stopped."""
        for element in elements:
            _, was_created = POI.objects.get_or_create(
                name=element["tags"]["name"],
                poi_type=self._poi_type,
                defaults={
                    "location": Point(element["lon"], element["lat"], srid=4326),
                    "altitude_m": _parse_altitude(element["tags"].get("ele")),
                },
            )
            self._fetched += 1
            if was_created:
                self._created += 1
        if elements:
            self.stdout.write(f"{self._indent()}+ {len(elements)} ({self._created} new so far)")

    def _run_queue(
        self,
        statement: str,
        queue: "deque[tuple[float, float, float, float, int]]",
        pending_path: Path,
    ) -> None:
        """Process regions breadth-first, quartering any that time out, until the queue drains."""
        while queue:
            if self._request_count >= MAX_TOTAL_REQUESTS:
                return
            west, south, east, north, depth = queue[0]
            self._depth = depth
            query = (
                f"[out:json][timeout:{QUERY_TIMEOUT_SECONDS}][bbox:{south},{west},{north},{east}];"
                f"{statement}out body;"
            )
            try:
                self._import_elements(self._fetch_elements(query))
            except _QueryTimedOut:
                bbox = (west, south, east, north)
                if depth >= MAX_SPLIT_DEPTH:
                    self.stdout.write(
                        self.style.ERROR(f"{self._indent()}x {_format_bbox(bbox)} still failing, deferring")
                    )
                    self._stuck.append(queue.popleft())
                else:
                    self.stdout.write(f"{self._indent()}~ {_format_bbox(bbox)} splitting...")
                    queue.popleft()
                    mid_lon, mid_lat = (west + east) / 2, (south + north) / 2
                    quadrants = [
                        (west, south, mid_lon, mid_lat, depth + 1),
                        (mid_lon, south, east, mid_lat, depth + 1),
                        (west, mid_lat, mid_lon, north, depth + 1),
                        (mid_lon, mid_lat, east, north, depth + 1),
                    ]
                    random.shuffle(quadrants)  # don't always starve the same quadrant on cutoff
                    queue.extend(quadrants)
            else:
                queue.popleft()
            pending_path.write_text(json.dumps(list(queue) + self._stuck))

    def _slow_down(self) -> None:
        """Double the inter-request pace after a 429, up to MAX_PACE_SECONDS."""
        new_pace = min(self._pace_seconds * 2, MAX_PACE_SECONDS)
        if new_pace > self._pace_seconds:
            self.stdout.write(
                self.style.WARNING(f"{self._indent()}! backing off to {new_pace:.0f}s between requests")
            )
        self._pace_seconds = new_pace

    def _speed_up(self) -> None:
        """Halve the inter-request pace after a clean success, down to BASE_PACE_SECONDS."""
        self._pace_seconds = max(self._pace_seconds / 2, BASE_PACE_SECONDS)

    def _fetch_elements(self, query: str) -> list[dict[str, Any]]:
        """POST to Overpass; only 429 is retried in place - everything else splits instead."""
        for attempt in range(1, MAX_ATTEMPTS + 1):
            if self._request_count:
                time.sleep(self._pace_seconds)
            self._request_count += 1
            try:
                response = requests.post(
                    OVERPASS_URL, data={"data": query}, headers=REQUEST_HEADERS,
                    timeout=HTTP_TIMEOUT_SECONDS,
                )
            except requests.exceptions.RequestException as exc:
                raise _QueryTimedOut(str(exc)) from None
            if response.ok:
                payload = response.json()
                if "remark" in payload:
                    raise _QueryTimedOut(payload["remark"])
                self._speed_up()
                elements: list[dict[str, Any]] = payload["elements"]
                return elements
            status_code = response.status_code
            if status_code != 429:
                if status_code < 500:
                    response.raise_for_status()
                raise _QueryTimedOut(f"HTTP {status_code}")
            self._slow_down()
            if attempt < MAX_ATTEMPTS:
                delay = _retry_after_seconds(response) or self._pace_seconds
                self.stdout.write(f"{self._indent()}! 429, retrying ({attempt}/{MAX_ATTEMPTS})...")
                time.sleep(delay)
        raise _QueryTimedOut(f"HTTP 429 after {MAX_ATTEMPTS} attempts")
