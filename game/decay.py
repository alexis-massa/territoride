from datetime import datetime, timedelta

from django.utils import timezone

from .models import POI, Activity, TerritoryCell

# Has no value after DECAY_THRESHOLD_DAYS days
DECAY_THRESHOLD_DAYS = 30


def elapsed_days(captured_at: datetime) -> int:
    """Whole days elapsed since a capture.

    Args:
        captured_at: When the capture happened.

    Returns:
        Days elapsed.
    """
    return (timezone.now().date() - captured_at.date()).days


def current_value(base_value: float, captured_at: datetime | None) -> float:
    """Linearly decayed value of a capture, based on time since capture.

    Args:
        base_value: The value at the moment of capture.
        captured_at: When it was captured, or None if unowned.

    Returns:
        base_value at capture time, decaying to 0 at DECAY_THRESHOLD_DAYS.
    """
    if captured_at is None:
        return 0
    remaining = max(0.0, 1 - elapsed_days(captured_at) / DECAY_THRESHOLD_DAYS)
    return base_value * remaining


def release_expired() -> int:
    """Return territory cells and POIs past the decay threshold to unowned,
    and delete activities old enough that nothing they captured survives.

    Returns:
        The number of cells and POIs released.
    """
    cutoff = timezone.now() - timedelta(days=DECAY_THRESHOLD_DAYS)
    cells = TerritoryCell.objects.filter(captured_at__lt=cutoff).update(
        owner=None, captured_by=None, captured_at=None
    )
    pois = POI.objects.filter(claimed_at__lt=cutoff).update(
        owner=None, claimed_by=None, claimed_at=None
    )
    Activity.objects.filter(recorded_at__lt=cutoff).delete()
    return cells + pois
