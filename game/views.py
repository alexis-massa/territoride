import json
import zlib
from typing import Any

from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, JsonResponse
from django.shortcuts import render

from .grid import cell_boundary, cells_in_bbox
from .models import Activity, TerritoryCell

BBOX_PARAM = "bbox"
PLAYER_COLORS = ["#2ecc71", "#e67e22", "#3498db", "#9b59b6", "#e74c3c", "#f1c40f"]


def map_view(request: HttpRequest) -> HttpResponse:
    """Render the main world map page.

    Args:
        request: The incoming request.

    Returns:
        The rendered map page.
    """
    return render(request, "game/map.html")


def grid_geojson_view(request: HttpRequest) -> HttpResponse:
    """Grid cells covering a viewport, as GeoJSON.

    Args:
        request: The incoming request; expects a "bbox" query param
            formatted as "min_lon,min_lat,max_lon,max_lat".

    Returns:
        A GeoJSON FeatureCollection, or 400 if bbox is missing/malformed.
    """
    if BBOX_PARAM not in request.GET:
        return HttpResponseBadRequest("Missing bbox param")
    try:
        min_lon, min_lat, max_lon, max_lat = (float(v) for v in request.GET[BBOX_PARAM].split(","))
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


def player_color(username: str) -> str:
    """Deterministic display color for a player.

    Args:
        username: The player's username.

    Returns:
        A hex color, stable across requests and process restarts.
    """
    return PLAYER_COLORS[zlib.crc32(username.encode()) % len(PLAYER_COLORS)]


def _territory_feature(cell: TerritoryCell) -> dict[str, Any]:
    """Build one territory GeoJSON feature.

    Args:
        cell: A captured cell (must have an owner).

    Returns:
        A GeoJSON Feature dict.
    """
    assert cell.owner is not None
    return {
        "type": "Feature",
        "geometry": {"type": "Polygon", "coordinates": [cell_boundary(cell.cell_id)]},
        "properties": {"owner": cell.owner.username, "color": player_color(cell.owner.username)},
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
