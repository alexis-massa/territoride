import h3

from .grid import RESOLUTION, cells_in_loop
from .models import Activity, TerritoryCell

# Strava trims ~200m off each end of a public activity to hide home
# addresses, so a real loop can show up to ~400m "open" - 500m covers
# that plus GPS noise.
LOOP_CLOSE_TOLERANCE_M = 500


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
