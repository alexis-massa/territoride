import math
import re
from datetime import datetime
from typing import Any

import gpxpy
import markdown
from django.conf import settings
from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.contrib.gis.db.models import Extent, LineStringField
from django.contrib.gis.geos import LineString, Polygon
from django.core.exceptions import PermissionDenied
from django.db.models import Exists, F, Func, OuterRef, Value
from django.http import HttpRequest, HttpResponse, HttpResponseBadRequest, JsonResponse
from django.shortcuts import redirect, render
from django.templatetags.static import static
from django.views.decorators.gzip import gzip_page
from django.views.decorators.http import require_POST

from accounts.models import User

from .capture import LOOP_CLOSE_TOLERANCE_M, POI_CAPTURE_RADIUS_M, capture_pois, capture_territory
from .decay import DECAY_THRESHOLD_DAYS, current_value, elapsed_days
from .grid import cell_boundary
from .models import POI, Activity, CellVisit, TerritoryCell
from .scoring import (
    DEFAULT_SPORT_MULTIPLIER,
    SPORT_MULTIPLIERS,
    TERRITORY_CELL_VALUE,
    leaderboard,
    player_color,
    player_colors,
)

BBOX_PARAM = "bbox"
BBOX_ERROR = "bbox must be min_lon,min_lat,max_lon,max_lat"

# "?mine=1" restricts a GeoJSON layer to the logged-in player's own captures and rides.
MINE_PARAM = "mine"

# Route simplification tolerance, as a fraction of the viewport width: sub-pixel detail
# is dropped server-side instead of being shipped to the browser and then discarded.
ROUTE_SIMPLIFY_DIVISOR = 1500


def _bbox_from_request(request: HttpRequest) -> tuple[float, float, float, float]:
    """Parse the "bbox" query param into (min_lon, min_lat, max_lon, max_lat)."""
    if BBOX_PARAM not in request.GET:
        raise ValueError("Missing bbox param")
    min_lon, min_lat, max_lon, max_lat = (float(v) for v in request.GET[BBOX_PARAM].split(","))
    if not all(math.isfinite(v) for v in (min_lon, min_lat, max_lon, max_lat)):
        raise ValueError("bbox values must be finite")
    return min_lon, min_lat, max_lon, max_lat


def _mine_user(request: HttpRequest) -> User | None:
    """The player a "mine" request is scoped to, or None for the shared world view.

    Raises PermissionDenied for "mine" without a login, rather than silently
    serving everyone's data to someone who asked for their own.
    """
    if request.GET.get(MINE_PARAM) != "1":
        return None
    if not request.user.is_authenticated:
        raise PermissionDenied("Log in to see only your own activity")
    assert isinstance(request.user, User)
    return request.user


def _bbox_polygon(bbox: tuple[float, float, float, float]) -> Polygon:
    polygon = Polygon.from_bbox(bbox)
    polygon.srid = 4326
    return polygon


def _user_territory_bbox(user: User) -> tuple[float, float, float, float] | None:
    """Bounding box covering a player's current cells and passes, or None if they hold nothing."""
    extents = [
        extent
        for extent in (
            TerritoryCell.objects.filter(owner=user).aggregate(e=Extent("centroid"))["e"],
            POI.objects.filter(owner=user).aggregate(e=Extent("location"))["e"],
        )
        if extent is not None
    ]
    if not extents:
        return None
    return (
        min(e[0] for e in extents),
        min(e[1] for e in extents),
        max(e[2] for e in extents),
        max(e[3] for e in extents),
    )


def map_view(request: HttpRequest) -> HttpResponse:
    """Render the main world map page."""
    default_bbox = None
    if request.user.is_authenticated:
        assert isinstance(request.user, User)
        default_bbox = _user_territory_bbox(request.user)
    return render(
        request,
        "game/map.html",
        {
            "decay_threshold_days": DECAY_THRESHOLD_DAYS,
            "default_bbox": default_bbox,
            "poi_types": POI.POIType.choices,
        },
    )


def _cell_feature(cell_id: str, username: str, color: str, at: datetime) -> dict[str, Any]:
    """Build one territory GeoJSON feature; "at" is when the cell was captured or visited."""
    return {
        "type": "Feature",
        "geometry": {"type": "Polygon", "coordinates": [cell_boundary(cell_id)]},
        "properties": {
            "owner": username,
            "color": color,
            "captured_at": at.isoformat(),
            "age_days": elapsed_days(at),
            "remaining": current_value(1.0, at),
        },
    }


def _territory_feature(cell: TerritoryCell, colors: dict[str, str]) -> dict[str, Any]:
    """Build one territory GeoJSON feature for a captured (owned) cell."""
    assert cell.owner is not None
    assert cell.captured_at is not None
    return _cell_feature(
        cell.cell_id, cell.owner.username, colors[cell.owner.username], cell.captured_at
    )


@gzip_page
def territory_geojson_view(request: HttpRequest) -> HttpResponse:
    """Captured territory cells centered in the viewport's "bbox" query param, as GeoJSON."""
    try:
        bbox = _bbox_polygon(_bbox_from_request(request))
    except ValueError:
        return HttpResponseBadRequest(BBOX_ERROR)

    if mine := _mine_user(request):
        # Every cell the player rode through recently, including ones since taken from them.
        color = player_color(mine.username)
        visits = CellVisit.objects.filter(user=mine, centroid__within=bbox)
        features = [
            _cell_feature(v.cell_id, mine.username, color, v.visited_at)
            for v in visits.only("cell_id", "visited_at")
        ]
        return JsonResponse({"type": "FeatureCollection", "features": features})

    cells = (
        TerritoryCell.objects.exclude(owner=None)
        .filter(centroid__within=bbox)
        .select_related("owner")
    )
    colors = player_colors()
    features = [_territory_feature(cell, colors) for cell in cells]
    return JsonResponse({"type": "FeatureCollection", "features": features})


@gzip_page
def routes_geojson_view(request: HttpRequest) -> HttpResponse:
    """Tracks of activities that captured a cell and cross the viewport's "bbox", as GeoJSON."""
    try:
        bounds = _bbox_from_request(request)
    except ValueError:
        return HttpResponseBadRequest(BBOX_ERROR)

    tolerance = (bounds[2] - bounds[0]) / ROUTE_SIMPLIFY_DIVISOR
    activities = Activity.objects.filter(track__intersects=_bbox_polygon(bounds))
    if mine := _mine_user(request):
        # Every ride of the player's, even ones whose tiles were since retaken: the point
        # is seeing where they have been, not what they still hold.
        activities = activities.filter(user=mine)
    else:
        activities = activities.filter(
            Exists(TerritoryCell.objects.filter(captured_by=OuterRef("pk")))
        )
    activities = (
        activities.annotate(
            simplified=Func(
                F("track"),
                Value(tolerance),
                function="ST_SimplifyPreserveTopology",
                output_field=LineStringField(srid=4326),
            ),
            owner_name=F("user__username"),
        )
        .only("name", "recorded_at")
    )
    colors = player_colors()
    features = [
        {
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": list(activity.simplified.coords)},
            "properties": {
                "name": activity.name,
                "owner": activity.owner_name,
                "color": colors[activity.owner_name],
                "age_days": elapsed_days(activity.recorded_at),
            },
        }
        for activity in activities
    ]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def _poi_feature(
    poi: POI, colors: dict[str, str], only_owner: User | None = None
) -> dict[str, Any]:
    """Build one POI GeoJSON feature; claim fields are only present on claimed POIs.

    With only_owner, POIs held by anyone else are shown as unclaimed.
    """
    properties: dict[str, Any] = {
        "name": poi.name,
        "poi_type": poi.poi_type,
        "poi_type_label": poi.get_poi_type_display(),
        "altitude_m": poi.altitude_m,
    }
    if poi.owner and poi.claimed_at and (only_owner is None or poi.owner_id == only_owner.pk):
        properties |= {
            "owner": poi.owner.username,
            "color": colors[poi.owner.username],
            "captured_at": poi.claimed_at.isoformat(),
            "age_days": elapsed_days(poi.claimed_at),
            "remaining": current_value(1.0, poi.claimed_at),
        }
    return {
        "type": "Feature",
        "geometry": {"type": "Point", "coordinates": [poi.location.x, poi.location.y]},
        "properties": properties,
    }


@gzip_page
def pois_geojson_view(request: HttpRequest) -> HttpResponse:
    """POIs (passes, summits) inside the viewport's "bbox" query param, as GeoJSON."""
    try:
        bbox = _bbox_polygon(_bbox_from_request(request))
    except ValueError:
        return HttpResponseBadRequest(BBOX_ERROR)

    pois = (
        POI.objects.filter(location__within=bbox)
        .select_related("owner")
        .only("name", "poi_type", "altitude_m", "location", "claimed_at", "owner__username")
    )
    colors = player_colors()
    mine = _mine_user(request)
    features = [_poi_feature(poi, colors, mine) for poi in pois]
    return JsonResponse({"type": "FeatureCollection", "features": features})


def _humanize_sport(sport_type: str) -> str:
    """Turn a Strava sport_type code (e.g. "TrailRun") into a readable label ("Trail Run")."""
    return re.sub(r"(?<=[a-z])(?=[A-Z])", " ", sport_type)


def rules_view(request: HttpRequest) -> HttpResponse:
    """Render the player-facing rules and security info page."""
    sport_multipliers = [("Cycling", DEFAULT_SPORT_MULTIPLIER)] + sorted(
        ((_humanize_sport(sport), multiplier) for sport, multiplier in SPORT_MULTIPLIERS.items()),
        key=lambda pair: pair[1],
    )
    return render(
        request,
        "game/rules.html",
        {
            "loop_tolerance_m": LOOP_CLOSE_TOLERANCE_M,
            "poi_radius_m": POI_CAPTURE_RADIUS_M,
            "decay_days": DECAY_THRESHOLD_DAYS,
            "cell_value": TERRITORY_CELL_VALUE,
            "default_multiplier": DEFAULT_SPORT_MULTIPLIER,
            "sport_multipliers": sport_multipliers,
        },
    )


def changelog_view(request: HttpRequest) -> HttpResponse:
    """Render the project changelog for players."""
    text = (settings.BASE_DIR / "CHANGELOG.md").read_text()
    return render(request, "game/changelog.html", {"changelog_html": markdown.markdown(text)})


def leaderboard_view(request: HttpRequest) -> HttpResponse:
    """Render the player rankings page."""
    return render(request, "game/leaderboard.html", {"rows": leaderboard(with_detail=True)})


@login_required
@require_POST
def import_activity_view(request: HttpRequest) -> HttpResponse:
    """Import an uploaded GPX file as an Activity and capture territory/POIs."""
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


def manifest_view(request: HttpRequest) -> JsonResponse:
    """Web app manifest, built with static() so icon URLs survive hashed storage in prod."""
    manifest = {
        "name": "TerritoRide",
        "short_name": "TerritoRide",
        "description": "Turn your rides and hikes into conquered territory.",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#f4f5f7",
        "theme_color": "#3b6fd6",
        "icons": [
            {"src": static("game/icons/icon-192.png"), "sizes": "192x192", "type": "image/png", "purpose": "any"},
            {"src": static("game/icons/icon-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "any"},
            {
                "src": static("game/icons/maskable-512.png"),
                "sizes": "512x512",
                "type": "image/png",
                "purpose": "maskable",
            },
        ],
    }
    return JsonResponse(manifest, content_type="application/manifest+json")


# Served from the root path (not /static/) so its default scope covers the whole app.
_SERVICE_WORKER_JS = b"""\
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (event) => event.respondWith(fetch(event.request)));
"""


def service_worker_view(request: HttpRequest) -> HttpResponse:
    """Minimal pass-through service worker, just enough to make the app installable."""
    return HttpResponse(_SERVICE_WORKER_JS, content_type="application/javascript")
