from datetime import datetime, timedelta

from django.utils import timezone

from .models import POI, Activity, TerritoryCell

# Has no value after DECAY_THRESHOLD_DAYS days
DECAY_THRESHOLD_DAYS = 30


def elapsed_days(captured_at: datetime) -> int:
    """Whole days elapsed since a capture."""
    return (timezone.now().date() - captured_at.date()).days


def current_value(base_value: float, captured_at: datetime | None) -> float:
    """base_value linearly decayed to 0 at DECAY_THRESHOLD_DAYS; 0 if unowned."""
    if captured_at is None:
        return 0
    remaining = max(0.0, 1 - elapsed_days(captured_at) / DECAY_THRESHOLD_DAYS)
    return base_value * remaining


def release_expired() -> int:
    """Return expired cells/POIs to unowned and delete activities past the threshold."""
    cutoff = timezone.now() - timedelta(days=DECAY_THRESHOLD_DAYS)
    cells = TerritoryCell.objects.filter(captured_at__lt=cutoff).update(
        owner=None, captured_by=None, captured_at=None
    )
    pois = POI.objects.filter(claimed_at__lt=cutoff).update(
        owner=None, claimed_by=None, claimed_at=None
    )
    Activity.objects.filter(recorded_at__lt=cutoff).delete()
    return cells + pois
