import zlib
from typing import Any

from django.db.models import Count

from accounts.models import User

from .decay import current_value, elapsed_days
from .models import POI, Activity, TerritoryCell

TERRITORY_CELL_VALUE = 10

PLAYER_COLORS = ["#2ecc71", "#e67e22", "#3498db", "#9b59b6", "#e74c3c", "#f1c40f"]

DEFAULT_SPORT_MULTIPLIER = 1.0
SPORT_MULTIPLIERS = {
    "Run": 2.0,
    "TrailRun": 2.0,
    "Walk": 5.0,
    "Hike": 5.0,
    "Swim": 8.0,
    "OpenWaterSwim": 8.0,
}

SWIM_SPORT_TYPES = {"Swim", "OpenWaterSwim"}


def sport_multiplier(activity: Activity | None) -> float:
    """Point multiplier for the sport that made a capture, 1x if unknown."""
    if activity is None:
        return DEFAULT_SPORT_MULTIPLIER
    return SPORT_MULTIPLIERS.get(activity.sport_type, DEFAULT_SPORT_MULTIPLIER)


def _territory_group_value(
    sport_type: str | None, distance_m: int | None, cell_count: int
) -> float:
    """Base value (before decay) for cells captured together by one activity.

    Swims are valued by distance alone, independent of cell_count - a swim
    holds at most a couple of tiles no matter how far it goes.
    """
    if sport_type in SWIM_SPORT_TYPES:
        return ((distance_m or 0) / 1000) * TERRITORY_CELL_VALUE * SPORT_MULTIPLIERS["Swim"]
    multiplier = SPORT_MULTIPLIERS.get(sport_type or "", DEFAULT_SPORT_MULTIPLIER)
    return TERRITORY_CELL_VALUE * multiplier * cell_count


def player_color(username: str) -> str:
    """Deterministic display color for a player, unique up to len(PLAYER_COLORS) players."""
    usernames = list(User.objects.order_by("id").values_list("username", flat=True))
    index = usernames.index(username) if username in usernames else zlib.crc32(username.encode())
    return PLAYER_COLORS[index % len(PLAYER_COLORS)]


def player_score(user: User) -> dict[str, int]:
    """Current point total for a player, broken down by source, decay applied.

    Trackless swims (no GPS fix at all) are counted under territory_points
    even though they hold no cell - see _territory_group_value, which values
    swims purely by distance regardless of cell count.
    """
    territory_groups = (
        TerritoryCell.objects.filter(owner=user)
        .values("captured_by__sport_type", "captured_by__distance_m", "captured_at")
        .annotate(cell_count=Count("cell_id"))
    )
    territory_points = sum(
        current_value(
            _territory_group_value(
                g["captured_by__sport_type"], g["captured_by__distance_m"], g["cell_count"]
            ),
            g["captured_at"],
        )
        for g in territory_groups
    )
    territory_points += sum(
        current_value(_territory_group_value(a.sport_type, a.distance_m, 0), a.recorded_at)
        for a in Activity.objects.filter(user=user, track__isnull=True)
    )
    poi_points = sum(
        current_value((poi.altitude_m or 0) * sport_multiplier(poi.claimed_by), poi.claimed_at)
        for poi in POI.objects.filter(owner=user).select_related("claimed_by")
    )
    return {
        "territory_points": round(territory_points),
        "poi_points": round(poi_points),
        "total": round(territory_points + poi_points),
    }


def score_split_pct(score: dict[str, int]) -> tuple[int, int]:
    """Territory/POI percentage split for a score's bar-chart display."""
    if not score["total"]:
        return 0, 0
    territory_pct = round(100 * score["territory_points"] / score["total"])
    return territory_pct, 100 - territory_pct


def player_capture_detail(user: User) -> dict[str, list[dict[str, Any]]]:
    """Per-ride/per-pass breakdown of a player's current holdings, newest first."""
    groups = TerritoryCell.objects.filter(owner=user).values(
        "captured_by__name", "captured_by__sport_type", "captured_by__distance_m", "captured_at"
    ).annotate(cell_count=Count("cell_id"))
    territory: list[dict[str, Any]] = []
    for g in groups:
        captured_at = g["captured_at"]
        assert captured_at is not None
        sport_type = g["captured_by__sport_type"]
        distance_m = g["captured_by__distance_m"]
        multiplier = SPORT_MULTIPLIERS.get(sport_type or "", DEFAULT_SPORT_MULTIPLIER)
        group_value = _territory_group_value(sport_type, distance_m, g["cell_count"])
        territory.append(
            {
                "activity_name": g["captured_by__name"] or "Unknown ride",
                "sport_type": sport_type,
                "multiplier": multiplier,
                "captured_at": captured_at,
                "age_days": elapsed_days(captured_at),
                "cell_count": g["cell_count"],
                "points": round(current_value(group_value, captured_at)),
            }
        )

    for activity in Activity.objects.filter(user=user, track__isnull=True):
        multiplier = SPORT_MULTIPLIERS.get(activity.sport_type or "", DEFAULT_SPORT_MULTIPLIER)
        group_value = _territory_group_value(activity.sport_type, activity.distance_m, 0)
        territory.append(
            {
                "activity_name": activity.name or "Unknown ride",
                "sport_type": activity.sport_type,
                "multiplier": multiplier,
                "captured_at": activity.recorded_at,
                "age_days": elapsed_days(activity.recorded_at),
                "cell_count": 0,
                "points": round(current_value(group_value, activity.recorded_at)),
            }
        )
    territory.sort(key=lambda t: t["captured_at"], reverse=True)

    pois: list[dict[str, Any]] = []
    for poi in POI.objects.filter(owner=user).select_related("claimed_by").order_by("-claimed_at"):
        assert poi.claimed_at is not None
        multiplier = sport_multiplier(poi.claimed_by)
        pois.append(
            {
                "name": poi.name,
                "altitude_m": poi.altitude_m,
                "sport_type": poi.claimed_by.sport_type if poi.claimed_by else "",
                "multiplier": multiplier,
                "claimed_at": poi.claimed_at,
                "age_days": elapsed_days(poi.claimed_at),
                "points": round(current_value((poi.altitude_m or 0) * multiplier, poi.claimed_at)),
            }
        )
    return {"territory": territory, "pois": pois}


def leaderboard(*, with_detail: bool = False) -> list[dict[str, Any]]:
    """Every player with any current ownership, ranked by score descending.

    with_detail also attaches each player's player_capture_detail() under
    "detail" - off by default since e.g. the profile page's rank lookup
    doesn't need it.
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
