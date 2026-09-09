import zlib
from typing import Any

from django.db.models import Count

from accounts.models import User

from .decay import current_value, elapsed_days
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


def player_capture_detail(user: User) -> dict[str, list[dict[str, Any]]]:
    """Per-ride/per-pass breakdown of a player's current holdings.

    Territory cells are grouped by the activity that captured them - an
    individual hex tile means nothing to a player, but "this ride holds N
    tiles worth P points" does.

    Args:
        user: The player to break down.

    Returns:
        "territory": groups sharing a capture, newest first, each with
            activity_name, captured_at, age_days, cell_count, points.
        "pois": individually claimed passes, newest first, each with name,
            altitude_m, claimed_at, age_days, points.
    """
    groups = (
        TerritoryCell.objects.filter(owner=user)
        .values("captured_by__name", "captured_at")
        .annotate(cell_count=Count("cell_id"))
        .order_by("-captured_at")
    )
    territory: list[dict[str, Any]] = []
    for g in groups:
        captured_at = g["captured_at"]
        assert captured_at is not None
        territory.append(
            {
                "activity_name": g["captured_by__name"] or "Unknown ride",
                "captured_at": captured_at,
                "age_days": elapsed_days(captured_at),
                "cell_count": g["cell_count"],
                "points": round(current_value(TERRITORY_CELL_VALUE, captured_at) * g["cell_count"]),
            }
        )

    pois: list[dict[str, Any]] = []
    for poi in POI.objects.filter(owner=user).order_by("-claimed_at"):
        assert poi.claimed_at is not None
        pois.append(
            {
                "name": poi.name,
                "altitude_m": poi.altitude_m,
                "claimed_at": poi.claimed_at,
                "age_days": elapsed_days(poi.claimed_at),
                "points": round(current_value(poi.altitude_m or 0, poi.claimed_at)),
            }
        )
    return {"territory": territory, "pois": pois}


def leaderboard(*, with_detail: bool = False) -> list[dict[str, Any]]:
    """Every player with any current ownership, ranked by score.

    Args:
        with_detail: Also attach each player's per-ride/per-pass breakdown
            (see player_capture_detail) under "detail". Off by default since
            callers like the profile page's rank lookup don't need it.

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
        row = {
            "username": user.username,
            "color": player_color(user.username),
            "cell_count": cell_count,
            "poi_count": poi_count,
            "territory_pct": territory_pct,
            "poi_pct": poi_pct,
            **score,
        }
        if with_detail:
            row["detail"] = player_capture_detail(user)
        rows.append(row)
    rows.sort(key=lambda row: row["total"], reverse=True)
    return rows
