from datetime import datetime
from typing import Any

from django.contrib.gis.geos import LineString

from accounts.models import User

from .capture import capture_pois, capture_territory
from .models import Activity


def decode_polyline(encoded: str) -> list[tuple[float, float]]:
    """Decode a Google encoded polyline into ordered (lat, lng) points.

    Args:
        encoded: The encoded polyline string, as returned by Strava.

    Returns:
        Ordered (lat, lng) points.
    """
    points = []
    index = lat = lng = 0
    while index < len(encoded):
        for is_lat in (True, False):
            shift = result = 0
            while True:
                byte = ord(encoded[index]) - 63
                index += 1
                result |= (byte & 0x1F) << shift
                shift += 5
                if byte < 0x20:
                    break
            delta = ~(result >> 1) if result & 1 else result >> 1
            if is_lat:
                lat += delta
            else:
                lng += delta
        points.append((lat / 1e5, lng / 1e5))
    return points


def import_from_strava(user: User, strava_activities: list[dict[str, Any]]) -> tuple[int, int, int]:
    """Create Activities from Strava activity summaries and capture territory/POIs.

    Skips activities already imported and those without GPS data (e.g. indoor
    workouts have no polyline).

    Args:
        user: The player these activities belong to.
        strava_activities: Raw activity summaries from Strava's list-activities API.

    Returns:
        (imported_count, cells_captured, pois_captured).
    """
    known_ids = set(
        Activity.objects.filter(strava_activity_id__isnull=False).values_list(
            "strava_activity_id", flat=True
        )
    )
    imported = cells = pois = 0
    for raw in strava_activities:
        strava_id = raw["id"]
        map_data = raw.get("map") or {}
        polyline = map_data.get("polyline") or map_data.get("summary_polyline")
        if strava_id in known_ids or not polyline:
            continue

        points = decode_polyline(polyline)
        if len(points) < 2:
            continue

        distance = raw.get("distance")
        activity = Activity.objects.create(
            user=user,
            strava_activity_id=strava_id,
            name=raw["name"],
            sport_type=raw.get("sport_type", ""),
            distance_m=round(distance) if distance is not None else None,
            track=LineString([(lng, lat) for lat, lng in points], srid=4326),
            recorded_at=datetime.fromisoformat(raw["start_date"]),
        )
        imported += 1
        cells += capture_territory(activity)
        pois += capture_pois(activity)
    return imported, cells, pois
