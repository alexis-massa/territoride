import json
from typing import Any

import gpxpy
import h3
import markdown
from django.conf import settings
from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.contrib.gis.geos import LineString, Polygon
from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, JsonResponse
from django.shortcuts import redirect, render
from django.views.decorators.http import require_POST

from accounts.models import User

from .capture import LOOP_CLOSE_TOLERANCE_M, POI_CAPTURE_RADIUS_M, capture_pois, capture_territory
from .decay import DECAY_THRESHOLD_DAYS, current_value, elapsed_days
from .grid import cell_boundary, cells_in_bbox
from .models import POI, Activity, TerritoryCell
from .scoring import TERRITORY_CELL_VALUE, leaderboard, player_color

BBOX_PARAM = "bbox"


def _bbox_from_request(request: HttpRequest) -> tuple[float, float, float, float]:
    """Parse the "bbox" query param into (min_lon, min_lat, max_lon, max_lat).

    Args:
        request: The incoming request.

    Raises:
        ValueError: If the param is missing or malformed.
    """
    if BBOX_PARAM not in request.GET:
        raise ValueError("Missing bbox param")
    min_lon, min_lat, max_lon, max_lat = (float(v) for v in request.GET[BBOX_PARAM].split(","))
    return min_lon, min_lat, max_lon, max_lat


def _user_territory_bbox(user: User) -> tuple[float, float, float, float] | None:
    """Bounding box covering a player's current cells and passes.

    Args:
        user: The player to center the map on.

    Returns:
        (min_lon, min_lat, max_lon, max_lat), or None if they hold nothing.
    """
    lats: list[float] = []
    lons: list[float] = []
    for cell_id in TerritoryCell.objects.filter(owner=user).values_list("cell_id", flat=True):
        lat, lng = h3.cell_to_latlng(cell_id)
        lats.append(lat)
        lons.append(lng)
    for location in POI.objects.filter(owner=user).values_list("location", flat=True):
        lons.append(location.x)
        lats.append(location.y)
    if not lats:
        return None
    return min(lons), min(lats), max(lons), max(lats)


def map_view(request: HttpRequest) -> HttpResponse:
    """Render the main world map page.

    Args:
        request: The incoming request.

    Returns:
        The rendered map page.
    """
    default_bbox = None
    if request.user.is_authenticated:
        assert isinstance(request.user, User)
        default_bbox = _user_territory_bbox(request.user)
    return render(
        request,
        "game/map.html",
        {"decay_threshold_days": DECAY_THRESHOLD_DAYS, "default_bbox": default_bbox},
    )


def grid_geojson_view(request: HttpRequest) -> HttpResponse:
    """Grid cells covering a viewport, as GeoJSON.

    Args:
        request: The incoming request; expects a "bbox" query param
            formatted as "min_lon,min_lat,max_lon,max_lat".

    Returns:
        A GeoJSON FeatureCollection, or 400 if bbox is missing/malformed.
    """
    try:
        min_lon, min_lat, max_lon, max_lat = _bbox_from_request(request)
    except ValueError:
        return HttpResponseBadRequest("bbox must be min_lon,min_lat,max_lon,max_lat")

    features = [
        {
            "type": "Feature",
            "geometry": {"type": "Polygon", "coordinates": [cell_boundary(cell)]},
            "properties": {},
        }
        for cell in cells_in_bbox(min_lon, min_lat, max_lon, max_lat)
    ]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def _territory_feature(cell: TerritoryCell) -> dict[str, Any]:
    """Build one territory GeoJSON feature.

    Args:
        cell: A captured cell (must have an owner).

    Returns:
        A GeoJSON Feature dict.
    """
    assert cell.owner is not None
    assert cell.captured_at is not None
    return {
        "type": "Feature",
        "geometry": {"type": "Polygon", "coordinates": [cell_boundary(cell.cell_id)]},
        "properties": {
            "owner": cell.owner.username,
            "color": player_color(cell.owner.username),
            "captured_at": cell.captured_at.isoformat(),
            "age_days": round(elapsed_days(cell.captured_at), 1),
            "remaining": current_value(1.0, cell.captured_at),
        },
    }


def territory_geojson_view(request: HttpRequest) -> HttpResponse:
    """Captured territory cells, as GeoJSON.

    Args:
        request: The incoming request.

    Returns:
        A GeoJSON FeatureCollection of owned cells.
    """
    cells = TerritoryCell.objects.exclude(owner=None).select_related("owner")
    features = [_territory_feature(cell) for cell in cells]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def routes_geojson_view(request: HttpRequest) -> HttpResponse:
    """Tracks of activities that have captured at least one cell, as GeoJSON.

    Args:
        request: The incoming request.

    Returns:
        A GeoJSON FeatureCollection of claiming routes.
    """
    activities = Activity.objects.filter(captured_cells__isnull=False).distinct()
    features = [
        {
            "type": "Feature",
            "geometry": json.loads(activity.track.geojson),
            "properties": {
                "name": activity.name,
                "owner": activity.user.username,
                "color": player_color(activity.user.username),
            },
        }
        for activity in activities
    ]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def _poi_feature(poi: POI) -> dict[str, Any]:
    """Build one POI GeoJSON feature.

    Args:
        poi: The point of interest to serialize.

    Returns:
        A GeoJSON Feature dict.
    """
    return {
        "type": "Feature",
        "geometry": json.loads(poi.location.geojson),
        "properties": {
            "name": poi.name,
            "altitude_m": poi.altitude_m,
            "owner": poi.owner.username if poi.owner else None,
            "color": player_color(poi.owner.username) if poi.owner else None,
            "captured_at": poi.claimed_at.isoformat() if poi.claimed_at else None,
            "age_days": round(elapsed_days(poi.claimed_at), 1) if poi.claimed_at else None,
            "remaining": current_value(1.0, poi.claimed_at) if poi.claimed_at else None,
        },
    }


def pois_geojson_view(request: HttpRequest) -> HttpResponse:
    """Mountain pass POIs covering a viewport, as GeoJSON.

    Args:
        request: The incoming request; expects a "bbox" query param
            formatted as "min_lon,min_lat,max_lon,max_lat".

    Returns:
        A GeoJSON FeatureCollection, or 400 if bbox is missing/malformed.
    """
    try:
        min_lon, min_lat, max_lon, max_lat = _bbox_from_request(request)
    except ValueError:
        return HttpResponseBadRequest("bbox must be min_lon,min_lat,max_lon,max_lat")

    bbox = Polygon.from_bbox((min_lon, min_lat, max_lon, max_lat))
    bbox.srid = 4326
    pois = POI.objects.filter(location__within=bbox).select_related("owner")
    features = [_poi_feature(poi) for poi in pois]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def rules_view(request: HttpRequest) -> HttpResponse:
    """Render the player-facing rules and security info page.

    Args:
        request: The incoming request.

    Returns:
        The rendered rules page.
    """
    return render(
        request,
        "game/rules.html",
        {
            "loop_tolerance_m": LOOP_CLOSE_TOLERANCE_M,
            "poi_radius_m": POI_CAPTURE_RADIUS_M,
            "decay_days": DECAY_THRESHOLD_DAYS,
            "cell_value": TERRITORY_CELL_VALUE,
        },
    )


def changelog_view(request: HttpRequest) -> HttpResponse:
    """Render the project changelog for players.

    Args:
        request: The incoming request.

    Returns:
        The rendered changelog page.
    """
    text = (settings.BASE_DIR / "CHANGELOG.md").read_text()
    return render(request, "game/changelog.html", {"changelog_html": markdown.markdown(text)})


def leaderboard_view(request: HttpRequest) -> HttpResponse:
    """Render the player rankings page.

    Args:
        request: The incoming request.

    Returns:
        The rendered leaderboard page.
    """
    return render(request, "game/leaderboard.html", {"rows": leaderboard()})


@login_required
@require_POST
def import_activity_view(request: HttpRequest) -> HttpResponse:
    """Import an uploaded GPX file as an Activity and capture territory/POIs.

    Args:
        request: The incoming request; expects a "gpx_file" upload.

    Returns:
        A redirect to the map, with a status message.
    """
    assert isinstance(request.user, User)
    gpx_file = request.FILES.get("gpx_file")
    if gpx_file is None:
        messages.error(request, "No file selected.")
        return redirect("map")

    try:
        gpx = gpxpy.parse(gpx_file.read().decode("utf-8"))
    except Exception:
        messages.error(request, "Couldn't read that file as GPX.")
        return redirect("map")

    points = [p for track in gpx.tracks for segment in track.segments for p in segment.points]
    if len(points) < 2:
        messages.error(request, "GPX file has fewer than 2 track points.")
        return redirect("map")
    if points[0].time is None:
        messages.error(request, "GPX file is missing activity date/time.")
        return redirect("map")

    activity = Activity.objects.create(
        user=request.user,
        name=gpx.tracks[0].name or gpx_file.name or "Untitled activity",
        track=LineString([(p.longitude, p.latitude) for p in points], srid=4326),
        recorded_at=points[0].time,
    )
    cells = capture_territory(activity)
    pois = capture_pois(activity)
    messages.success(request, f"Imported {activity.name!r}: captured {cells} cells, {pois} passes.")
    return redirect("map")
