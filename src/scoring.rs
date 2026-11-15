//! Points: what a player's current holdings are worth, decay applied, and the leaderboard.
//! Same rules as the Python version's `game/scoring.py`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pi_common::ApiResult;
use serde::Serialize;
use sqlx::PgPool;

use crate::decay::{current_value, elapsed_days};

pub const TERRITORY_CELL_VALUE: f64 = 10.0;
pub const PLAYER_COLORS: [&str; 6] = [
    "#2ecc71", "#e67e22", "#3498db", "#9b59b6", "#e74c3c", "#f1c40f",
];
pub const DEFAULT_SPORT_MULTIPLIER: f64 = 1.0;
/// Points multiplier per Strava sport type; anything else (cycling) uses [`DEFAULT_SPORT_MULTIPLIER`].
pub const SPORT_MULTIPLIERS: [(&str, f64); 6] = [
    ("Run", 2.0),
    ("TrailRun", 2.0),
    ("Walk", 5.0),
    ("Hike", 5.0),
    ("Swim", 8.0),
    ("OpenWaterSwim", 8.0),
];
pub const SWIM_SPORT_TYPES: [&str; 2] = ["Swim", "OpenWaterSwim"];

pub fn sport_multiplier(sport_type: &str) -> f64 {
    SPORT_MULTIPLIERS
        .iter()
        .find(|(s, _)| *s == sport_type)
        .map_or(DEFAULT_SPORT_MULTIPLIER, |(_, m)| *m)
}

/// Base value (before decay) of the cells one activity captured together. Swims are valued by distance alone:
/// a swim holds at most a couple of tiles however far it goes.
fn group_value(sport_type: &str, distance_m: Option<i32>, cell_count: i64) -> f64 {
    if SWIM_SPORT_TYPES.contains(&sport_type) {
        return f64::from(distance_m.unwrap_or(0)) / 1000.0
            * TERRITORY_CELL_VALUE
            * sport_multiplier("Swim");
    }
    TERRITORY_CELL_VALUE * sport_multiplier(sport_type) * cell_count as f64
}

/// Display colour of every player: by order of account creation, repeating after six players.
pub async fn player_colors(pool: &PgPool) -> ApiResult<HashMap<String, String>> {
    let names = sqlx::query_scalar!("SELECT username FROM accounts_user ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(names
        .into_iter()
        .enumerate()
        .map(|(i, n)| (n, PLAYER_COLORS[i % PLAYER_COLORS.len()].to_owned()))
        .collect())
}

/// One player's colour; a stable one by name for a name that is not in `colors`.
pub fn color_of(colors: &HashMap<String, String>, username: &str) -> String {
    colors.get(username).cloned().unwrap_or_else(|| {
        PLAYER_COLORS[crc32fast::hash(username.as_bytes()) as usize % PLAYER_COLORS.len()]
            .to_owned()
    })
}

/// Python's `round()`: halves go to the even neighbour.
fn round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Score {
    pub territory_points: i64,
    pub poi_points: i64,
    pub total: i64,
}

/// Territory/POI percentage split for a score's bar-chart display.
pub fn score_split_pct(score: &Score) -> (i64, i64) {
    if score.total == 0 {
        return (0, 0);
    }
    let territory = round(100.0 * score.territory_points as f64 / score.total as f64);
    (territory, 100 - territory)
}

/// A player's current points by source, decay applied. Swim points come from the player's swim activities, not
/// from the tiles they hold: a swim scores by distance whether or not it still holds its tile.
pub async fn player_score(pool: &PgPool, user_id: i64, now: DateTime<Utc>) -> ApiResult<Score> {
    let groups = sqlx::query!(
        r#"SELECT COALESCE(a.sport_type, '') AS "sport_type!", a.distance_m, c.captured_at, count(*) AS "n!"
           FROM game_territorycell c LEFT JOIN game_activity a ON a.id = c.captured_by_id
           WHERE c.owner_id = $1 AND COALESCE(a.sport_type, '') <> ALL($2)
           GROUP BY a.sport_type, a.distance_m, c.captured_at"#,
        user_id,
        &SWIM_SPORT_TYPES.map(String::from)
    )
    .fetch_all(pool)
    .await?;
    let mut territory: f64 = groups
        .iter()
        .map(|g| {
            current_value(
                group_value(&g.sport_type, g.distance_m, g.n),
                g.captured_at,
                now,
            )
        })
        .sum();
    territory += swims(pool, user_id)
        .await?
        .iter()
        .map(|s| {
            current_value(
                group_value(&s.sport_type, s.distance_m, 0),
                Some(s.recorded_at),
                now,
            )
        })
        .sum::<f64>();
    let pois = sqlx::query!(
        "SELECT altitude_m, claimed_at FROM game_poi WHERE owner_id = $1",
        user_id
    )
    .fetch_all(pool)
    .await?;
    let poi: f64 = pois
        .iter()
        .map(|p| current_value(f64::from(p.altitude_m.unwrap_or(0)), p.claimed_at, now))
        .sum();
    Ok(Score {
        territory_points: round(territory),
        poi_points: round(poi),
        total: round(territory + poi),
    })
}

struct Swim {
    id: i64,
    name: String,
    sport_type: String,
    distance_m: Option<i32>,
    recorded_at: DateTime<Utc>,
}

async fn swims(pool: &PgPool, user_id: i64) -> ApiResult<Vec<Swim>> {
    Ok(sqlx::query_as!(
        Swim,
        "SELECT id, name, sport_type, distance_m, recorded_at FROM game_activity
         WHERE user_id = $1 AND sport_type = ANY($2) ORDER BY id",
        user_id,
        &SWIM_SPORT_TYPES.map(String::from)
    )
    .fetch_all(pool)
    .await?)
}

#[derive(Debug, Serialize)]
pub struct TerritoryDetail {
    pub activity_name: String,
    pub sport_type: String,
    pub multiplier: f64,
    pub captured_at: DateTime<Utc>,
    pub age_days: i64,
    pub cell_count: i64,
    pub points: i64,
}

#[derive(Debug, Serialize)]
pub struct PoiDetail {
    pub name: String,
    pub poi_type_label: &'static str,
    pub altitude_m: Option<i32>,
    pub sport_type: String,
    pub claimed_at: DateTime<Utc>,
    pub age_days: i64,
    pub points: i64,
}

#[derive(Debug, Serialize)]
pub struct CaptureDetail {
    pub territory: Vec<TerritoryDetail>,
    pub pois: Vec<PoiDetail>,
}

pub fn poi_type_label(poi_type: &str) -> &'static str {
    match poi_type {
        "summit" => "Summit",
        _ => "Mountain pass",
    }
}

/// Per-ride and per-pass breakdown of a player's current holdings, newest first.
pub async fn player_capture_detail(
    pool: &PgPool,
    user_id: i64,
    now: DateTime<Utc>,
) -> ApiResult<CaptureDetail> {
    let groups = sqlx::query!(
        r#"SELECT a.name AS "name?", COALESCE(a.sport_type, '') AS "sport_type!", a.distance_m,
                  c.captured_at AS "captured_at!", count(*) AS "n!"
           FROM game_territorycell c LEFT JOIN game_activity a ON a.id = c.captured_by_id
           WHERE c.owner_id = $1 AND COALESCE(a.sport_type, '') <> ALL($2)
           GROUP BY a.name, a.sport_type, a.distance_m, c.captured_at"#,
        user_id,
        &SWIM_SPORT_TYPES.map(String::from)
    )
    .fetch_all(pool)
    .await?;
    let mut territory: Vec<TerritoryDetail> = groups
        .into_iter()
        .map(|g| TerritoryDetail {
            activity_name: g
                .name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Unknown ride".into()),
            multiplier: sport_multiplier(&g.sport_type),
            age_days: elapsed_days(g.captured_at, now),
            points: round(current_value(
                group_value(&g.sport_type, g.distance_m, g.n),
                Some(g.captured_at),
                now,
            )),
            sport_type: g.sport_type,
            captured_at: g.captured_at,
            cell_count: g.n,
        })
        .collect();
    for s in swims(pool, user_id).await? {
        let held = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM game_territorycell WHERE captured_by_id = $1 AND owner_id = $2"#,
            s.id,
            user_id
        )
        .fetch_one(pool)
        .await?;
        territory.push(TerritoryDetail {
            activity_name: if s.name.is_empty() {
                "Unknown ride".into()
            } else {
                s.name.clone()
            },
            multiplier: sport_multiplier(&s.sport_type),
            age_days: elapsed_days(s.recorded_at, now),
            points: round(current_value(
                group_value(&s.sport_type, s.distance_m, 0),
                Some(s.recorded_at),
                now,
            )),
            sport_type: s.sport_type.clone(),
            captured_at: s.recorded_at,
            cell_count: held,
        });
    }
    territory.sort_by_key(|t| std::cmp::Reverse(t.captured_at));
    let pois = sqlx::query!(
        r#"SELECT p.name, p.poi_type, p.altitude_m, p.claimed_at AS "claimed_at!", a.sport_type AS "sport_type?"
           FROM game_poi p LEFT JOIN game_activity a ON a.id = p.claimed_by_id
           WHERE p.owner_id = $1 AND p.claimed_at IS NOT NULL ORDER BY p.claimed_at DESC"#,
        user_id
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|p| PoiDetail {
        name: p.name,
        poi_type_label: poi_type_label(&p.poi_type),
        altitude_m: p.altitude_m,
        sport_type: p.sport_type.unwrap_or_default(),
        age_days: elapsed_days(p.claimed_at, now),
        points: round(current_value(f64::from(p.altitude_m.unwrap_or(0)), Some(p.claimed_at), now)),
        claimed_at: p.claimed_at,
    })
    .collect();
    Ok(CaptureDetail { territory, pois })
}

#[derive(Debug, Serialize)]
pub struct LeaderboardRow {
    pub username: String,
    pub color: String,
    pub cell_count: i64,
    pub poi_count: i64,
    pub territory_pct: i64,
    pub poi_pct: i64,
    #[serde(flatten)]
    pub score: Score,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<CaptureDetail>,
}

/// Every player with any current ownership, ranked by score, highest first.
pub async fn leaderboard(
    pool: &PgPool,
    now: DateTime<Utc>,
    with_detail: bool,
) -> ApiResult<Vec<LeaderboardRow>> {
    let colors = player_colors(pool).await?;
    let users = sqlx::query!(
        r#"SELECT u.id, u.username,
                  (SELECT count(*) FROM game_territorycell WHERE owner_id = u.id) AS "cells!",
                  (SELECT count(*) FROM game_poi WHERE owner_id = u.id) AS "pois!",
                  EXISTS (SELECT 1 FROM game_activity WHERE user_id = u.id AND sport_type = ANY($1)) AS "swims!"
           FROM accounts_user u ORDER BY u.id"#,
        &SWIM_SPORT_TYPES.map(String::from)
    )
    .fetch_all(pool)
    .await?;
    let mut rows = Vec::new();
    for u in users {
        if u.cells == 0 && u.pois == 0 && !u.swims {
            continue;
        }
        let score = player_score(pool, u.id, now).await?;
        let (territory_pct, poi_pct) = score_split_pct(&score);
        rows.push(LeaderboardRow {
            color: color_of(&colors, &u.username),
            username: u.username,
            cell_count: u.cells,
            poi_count: u.pois,
            territory_pct,
            poi_pct,
            score,
            detail: if with_detail {
                Some(player_capture_detail(pool, u.id, now).await?)
            } else {
                None
            },
        });
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.score.total));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_the_rules() {
        assert_eq!(group_value("", Some(40_000), 12), 120.0); // cycling: 10 per cell
        assert_eq!(group_value("Run", Some(5_000), 4), 80.0);
        assert_eq!(group_value("Hike", None, 3), 150.0);
        assert_eq!(
            group_value("Swim", Some(1_500), 7),
            120.0,
            "swims count distance, not cells"
        );
        assert_eq!(group_value("OpenWaterSwim", None, 1), 0.0);
    }

    #[test]
    fn rounding_and_split_match_python() {
        assert_eq!(
            (round(0.5), round(1.5), round(2.5), round(-0.5)),
            (0, 2, 2, 0)
        );
        let s = Score {
            territory_points: 1,
            poi_points: 2,
            total: 3,
        };
        assert_eq!(score_split_pct(&s), (33, 67));
        assert_eq!(
            score_split_pct(&Score {
                territory_points: 0,
                poi_points: 0,
                total: 0
            }),
            (0, 0)
        );
    }

    #[test]
    fn colors_cycle_and_fall_back_to_a_stable_one() {
        let colors: HashMap<String, String> =
            [("ann".to_owned(), PLAYER_COLORS[0].to_owned())].into();
        assert_eq!(color_of(&colors, "ann"), PLAYER_COLORS[0]);
        assert_eq!(color_of(&colors, "nobody"), color_of(&colors, "nobody"));
    }
}
