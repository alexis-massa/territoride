import h3
from django.contrib.gis.db.models.functions import Distance
from django.contrib.gis.measure import D

from .grid import RESOLUTION, cells_in_loop
from .models import POI, Activity, TerritoryCell

# Strava trims ~200m at start/end, so a loop can show up to ~400m "open".
LOOP_CLOSE_TOLERANCE_M = 500

POI_CAPTURE_RADIUS_M = 100


def is_loop(points: list[tuple[float, float]]) -> bool:
    """Whether a track's endpoints are within LOOP_CLOSE_TOLERANCE_M of each other."""
    distance: float = h3.great_circle_distance(points[0], points[-1], unit="m")
    return distance <= LOOP_CLOSE_TOLERANCE_M


def touched_cells(points: list[tuple[float, float]]) -> set[str]:
    """H3 cells a track passes through."""
    return {h3.latlng_to_cell(lat, lng, RESOLUTION) for lat, lng in points}


def captured_cells(points: list[tuple[float, float]]) -> set[str]:
    """Cells a track captures: touched cells, plus enclosed ones if it's a loop."""
    cells = touched_cells(points)
    if is_loop(points):
        cells |= cells_in_loop(points)
    return cells


def capture_point(activity: Activity, lat: float, lng: float) -> int:
    """Claim the single cell at a point; always captures 1 cell."""
    cell_id = h3.latlng_to_cell(lat, lng, RESOLUTION)
    TerritoryCell.objects.update_or_create(
        cell_id=cell_id,
        defaults={
            "owner": activity.user,
            "captured_by": activity,
            "captured_at": activity.recorded_at,
        },
    )
    return 1


def capture_territory(activity: Activity) -> int:
    """Claim every cell an activity's track captures; returns the count captured."""
    assert activity.track is not None
    points = [(lat, lng) for lng, lat in activity.track.coords]
    cells = captured_cells(points)
    for cell_id in cells:
        TerritoryCell.objects.update_or_create(
            cell_id=cell_id,
            defaults={
                "owner": activity.user,
                "captured_by": activity,
                "captured_at": activity.recorded_at,
            },
        )
    return len(cells)


def capture_pois(activity: Activity) -> int:
    """Claim every nearby POI; returns the count captured."""
    nearby = POI.objects.annotate(distance=Distance("location", activity.track)).filter(
        distance__lte=D(m=POI_CAPTURE_RADIUS_M)  # type: ignore[misc]  # django-stubs wants a float here, but a Distance object is correct
    )
    return nearby.update(owner=activity.user, claimed_by=activity, claimed_at=activity.recorded_at)


def recapture_all() -> tuple[int, int]:
    """Replay capture for every activity oldest-first, so the most recent through each
    cell/POI ends up owning it. Needed after POI/scoring changes or an activity deletion.
    """
    activities = list(Activity.objects.order_by("recorded_at"))
    cells = pois = 0
    for activity in activities:
        if activity.track is None:
            continue
        points = [(lat, lng) for lng, lat in activity.track.coords]
        if len(set(points)) == 1:
            cells += capture_point(activity, *points[0])
        else:
            cells += capture_territory(activity)
        pois += capture_pois(activity)
    return cells, pois


def release_activity(activity: Activity) -> None:
    """Release everything an activity holds, delete it, then let other activities reclaim."""
    TerritoryCell.objects.filter(captured_by=activity).update(
        owner=None, captured_by=None, captured_at=None
    )
    POI.objects.filter(claimed_by=activity).update(owner=None, claimed_by=None, claimed_at=None)
    activity.delete()
    recapture_all()
