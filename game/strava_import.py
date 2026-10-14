import logging
from datetime import datetime
from typing import Any

from django.contrib.gis.geos import LineString

from accounts.models import User

from .capture import capture_point, capture_pois, capture_territory
from .models import Activity
from .scoring import SWIM_SPORT_TYPES

logger = logging.getLogger("game.strava_import")


def decode_polyline(encoded: str) -> list[tuple[float, float]]:
    """Decode a Google encoded polyline (Strava's format) into ordered (lat, lng) points."""
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
    """Create Activities from Strava summaries and capture territory/POIs.

    Skips already-imported activities and, for non-swim sports, ones with no
    location data at all. A start_latlng but no polyline (e.g. a pool swim -
    signal before/after but not underwater) still captures its one tile.
    A swim with no location data at all still gets imported and scored by
    distance, it just doesn't hold a tile.
    """
    known_ids = set(
        Activity.objects.filter(strava_activity_id__isnull=False).values_list(
            "strava_activity_id", flat=True
        )
    )
    imported = cells = pois = 0
    for raw in strava_activities:
        strava_id = raw["id"]
        if strava_id in known_ids:
            _sync_metadata(strava_id, raw)
            continue

        sport_type = raw.get("sport_type", "")
        map_data = raw.get("map") or {}
        polyline = map_data.get("polyline") or map_data.get("summary_polyline")
        point_only = False
        if polyline:
            points = decode_polyline(polyline)
            if len(points) < 2:
                continue
        else:
            start_latlng = raw.get("start_latlng")
            if start_latlng and len(start_latlng) == 2:
                lat, lng = start_latlng
                points = [(lat, lng), (lat, lng)]
                point_only = True
            elif sport_type in SWIM_SPORT_TYPES:
                points = None
            else:
                continue

        distance = raw.get("distance")
        activity = Activity.objects.create(
            user=user,
            strava_activity_id=strava_id,
            name=raw["name"],
            sport_type=sport_type,
            distance_m=round(distance) if distance is not None else None,
            track=LineString([(lng, lat) for lat, lng in points], srid=4326) if points else None,
            recorded_at=datetime.fromisoformat(raw["start_date"]),
        )
        imported += 1
        if points is None:
            activity_cells = 0
        elif point_only:
            lat, lng = points[0]
            activity_cells = capture_point(activity, lat, lng)
        else:
            activity_cells = capture_territory(activity)
        cells += activity_cells
        activity_pois = capture_pois(activity) if activity.track is not None else 0
        pois += activity_pois
        logger.info(
            "Imported %r (%s) for %s: %d cell(s), %d POI(s) captured",
            activity.name,
            sport_type,
            user.username,
            activity_cells,
            activity_pois,
        )
    return imported, cells, pois


def _sync_metadata(strava_id: int, raw: dict[str, Any]) -> None:
    """Keep editable fields in sync on a Strava update event, without re-running capture."""
    distance = raw.get("distance")
    Activity.objects.filter(strava_activity_id=strava_id).update(
        name=raw["name"],
        sport_type=raw.get("sport_type", ""),
        distance_m=round(distance) if distance is not None else None,
    )
    logger.info("Synced metadata for Strava activity %s: %r", strava_id, raw["name"])
