//! Public game data: the leaderboard, the rules numbers, the changelog.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use pi_common::ApiResult;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::capture;
use crate::decay::DECAY_THRESHOLD_DAYS;
use crate::scoring;

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

pub(super) async fn leaderboard(
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<scoring::LeaderboardRow>>> {
    Ok(Json(scoring::leaderboard(&pool, Utc::now(), true).await?))
}

/// "TrailRun" → "Trail Run".
fn humanize_sport(sport_type: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in sport_type.chars() {
        if c.is_uppercase() && previous_lower {
            out.push(' ');
        }
        previous_lower = c.is_lowercase();
        out.push(c);
    }
    out
}

/// The numbers the rules page and the map legend quote.
pub(super) async fn rules() -> Json<Value> {
    let mut multipliers: Vec<(String, f64)> =
        vec![("Cycling".into(), scoring::DEFAULT_SPORT_MULTIPLIER)];
    multipliers.extend(
        scoring::SPORT_MULTIPLIERS
            .iter()
            .map(|(s, m)| (humanize_sport(s), *m)),
    );
    multipliers.sort_by(|a, b| a.1.total_cmp(&b.1));
    Json(json!({
        "loop_tolerance_m": capture::LOOP_CLOSE_TOLERANCE_M,
        "poi_radius_m": capture::POI_CAPTURE_RADIUS_M,
        "decay_days": DECAY_THRESHOLD_DAYS,
        "cell_value": scoring::TERRITORY_CELL_VALUE,
        "default_multiplier": scoring::DEFAULT_SPORT_MULTIPLIER,
        "sport_multipliers": multipliers,
    }))
}

pub(super) async fn changelog() -> Json<Value> {
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(CHANGELOG));
    Json(json!({ "html": html }))
}
