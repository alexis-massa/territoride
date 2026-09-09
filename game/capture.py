import h3
from django.contrib.gis.db.models.functions import Distance
from django.contrib.gis.measure import D

from .grid import RESOLUTION, cells_in_loop
from .models import POI, Activity, TerritoryCell

# Strava trims ~200m at start/end, so a loop can show up to ~400m "open"
# 500m covers GPS noise.
LOOP_CLOSE_TOLERANCE_M = 500

# Cover GPS noise
POI_CAPTURE_RADIUS_M = 100


def is_loop(points: list[tuple[float, float]]) -> bool:
    """Whether a track's endpoints are close enough to count as a loop.

    Args:
        points: Ordered (lat, lng) points along the track.

    Returns:
        True if the endpoints are within LOOP_CLOSE_TOLERANCE_M of each other.
    """
    distance: float = h3.great_circle_distance(points[0], points[-1], unit="m")
    return distance <= LOOP_CLOSE_TOLERANCE_M


def touched_cells(points: list[tuple[float, float]]) -> set[str]:
    """H3 cells a track passes through.

    Args:
        points: Ordered (lat, lng) points along the track.

    Returns:
        Cell indexes.
    """
    return {h3.latlng_to_cell(lat, lng, RESOLUTION) for lat, lng in points}


def captured_cells(points: list[tuple[float, float]]) -> set[str]:
    """Cells a track captures: touched cells, plus enclosed ones if it's a loop.

    Args:
        points: Ordered (lat, lng) points along the track.

    Returns:
        Cell indexes.
    """
    cells = touched_cells(points)
    if is_loop(points):
        cells |= cells_in_loop(points)
    return cells


def capture_territory(activity: Activity) -> int:
    """Claim every cell an activity captures, for that activity's owner.

    Args:
        activity: The activity whose track determines what it captures.

    Returns:
        The number of cells captured.
    """
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
    """Claim every POI an activity's track passes within range of.

    Loops and non-loops are treated the same here - a POI is claimed by
    proximity to the track, never by enclosure.

    Args:
        activity: The activity whose track determines what it captures.

    Returns:
        The number of POIs captured.
    """
    nearby = POI.objects.annotate(distance=Distance("location", activity.track)).filter(
        distance__lte=D(m=POI_CAPTURE_RADIUS_M)  # type: ignore[misc]  # django-stubs wants a float here, but a Distance object is correct
    )
    return nearby.update(owner=activity.user, claimed_by=activity, claimed_at=activity.recorded_at)


def recapture_all() -> tuple[int, int]:
    """Replay capture for every activity, oldest first.

    Needed after POIs are added, capture logic changes, or a player's
    activities are deleted - so any surviving activity through the same
    ground reclaims it instead of leaving it stuck at whatever was last
    written.

    Returns:
        (cells_captured, pois_captured) totals across every activity.
    """
    activities = list(Activity.objects.order_by("recorded_at"))
    cells = pois = 0
    for activity in activities:
        cells += capture_territory(activity)
        pois += capture_pois(activity)
    return cells, pois


def release_activity(activity: Activity) -> None:
    """Release everything an activity holds, then delete it.

    Deleting just the Activity row (e.g. via cascade/SET_NULL) only clears
    captured_by/claimed_by - owner points at the player directly, who still
    exists, so cells/POIs would otherwise stay stuck showing an owner with
    nothing behind the claim. recapture_all() afterward lets any other
    surviving activity that also touched the same ground reclaim it.

    Args:
        activity: The activity to remove.
    """
    TerritoryCell.objects.filter(captured_by=activity).update(
        owner=None, captured_by=None, captured_at=None
    )
    POI.objects.filter(claimed_by=activity).update(owner=None, claimed_by=None, claimed_at=None)
    activity.delete()
    recapture_all()
