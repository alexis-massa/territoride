import zlib
from typing import Any

from accounts.models import User

from .decay import current_value
from .models import POI, TerritoryCell

# Flat value per owned cell. POIs are valued by altitude instead of a flat
# number - a bigger pass is worth more, and we already have real elevation
# data for most of them.
TERRITORY_CELL_VALUE = 10

PLAYER_COLORS = ["#2ecc71", "#e67e22", "#3498db", "#9b59b6", "#e74c3c", "#f1c40f"]


def player_color(username: str) -> str:
    """Deterministic display color for a player.

    Args:
        username: The player's username.

    Returns:
        A hex color, stable across requests and process restarts.
    """
    return PLAYER_COLORS[zlib.crc32(username.encode()) % len(PLAYER_COLORS)]


def player_score(user: User) -> dict[str, int]:
    """Current point total for a player, broken down by source.

    Reflects decay (see decay.py) but not exploration bonuses - those need
    capture history, which isn't tracked yet.

    Args:
        user: The player to score.

    Returns:
        A dict with "territory_points", "poi_points", and "total".
    """
    territory_points = sum(
        current_value(TERRITORY_CELL_VALUE, cell.captured_at)
        for cell in TerritoryCell.objects.filter(owner=user)
    )
    poi_points = sum(
        current_value(poi.altitude_m or 0, poi.claimed_at) for poi in POI.objects.filter(owner=user)
    )
    return {
        "territory_points": round(territory_points),
        "poi_points": round(poi_points),
        "total": round(territory_points + poi_points),
    }


def score_split_pct(score: dict[str, int]) -> tuple[int, int]:
    """Territory/POI percentage split, for a score's bar-chart display.

    Args:
        score: A player_score() result.

    Returns:
        (territory_pct, poi_pct), both 0-100, always summing to 100.
    """
    if not score["total"]:
        return 0, 0
    territory_pct = round(100 * score["territory_points"] / score["total"])
    return territory_pct, 100 - territory_pct


def leaderboard() -> list[dict[str, Any]]:
    """Every player with any current ownership, ranked by score.

    Returns:
        Dicts with username, color, cell_count, poi_count, territory_points,
        poi_points, total, territory_pct, poi_pct - sorted by total
        descending.
    """
    rows: list[dict[str, Any]] = []
    for user in User.objects.all():
        cell_count = TerritoryCell.objects.filter(owner=user).count()
        poi_count = POI.objects.filter(owner=user).count()
        if not cell_count and not poi_count:
            continue
        score = player_score(user)
        territory_pct, poi_pct = score_split_pct(score)
        rows.append(
            {
                "username": user.username,
                "color": player_color(user.username),
                "cell_count": cell_count,
                "poi_count": poi_count,
                "territory_pct": territory_pct,
                "poi_pct": poi_pct,
                **score,
            }
        )
    rows.sort(key=lambda row: row["total"], reverse=True)
    return rows
