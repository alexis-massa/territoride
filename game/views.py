from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, JsonResponse
from django.shortcuts import render

from .grid import cell_boundary, cells_in_bbox

BBOX_PARAM = "bbox"


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
