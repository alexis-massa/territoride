from accounts.models import User

from .decay import current_value
from .models import POI, TerritoryCell

# Flat value per owned cell. POIs are valued by altitude instead of a flat
# number - a bigger pass is worth more, and we already have real elevation
# data for most of them.
TERRITORY_CELL_VALUE = 10


def player_score(user: User) -> dict[str, int]:
    """Current point total for a player, broken down by source.

    Reflects decay (see decay.py) but not exploration bonuses - those need
    capture history, which isn't tracked yet.

    Args:
        user: The player to score.

    Returns:
        A dict with "territory", "pois", and "total" point counts.
    """
    territory = sum(
        current_value(TERRITORY_CELL_VALUE, cell.captured_at)
        for cell in TerritoryCell.objects.filter(owner=user)
    )
    pois = sum(
        current_value(poi.altitude_m or 0, poi.claimed_at) for poi in POI.objects.filter(owner=user)
    )
    return {"territory": round(territory), "pois": round(pois), "total": round(territory + pois)}
