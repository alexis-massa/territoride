//! The logged-in player: identity, profile, logout, account deletion.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use pi_common::ApiResult;
use pi_common::auth::Session;
use serde::Serialize;
use sqlx::PgPool;

use super::AppState;
use crate::{scoring, users};

/// The logged-in player, if any, and where the map should open.
#[derive(Serialize)]
pub(super) struct Me {
    user: Option<Player>,
    default_bbox: Option<[f64; 4]>,
}

#[derive(Serialize)]
struct Player {
    username: String,
    color: String,
    csrf_token: String,
    strava_connected: bool,
}

pub(super) async fn me(
    State(pool): State<PgPool>,
    session: Option<Session>,
) -> ApiResult<Json<Me>> {
    let Some(session) = session else {
        return Ok(Json(Me {
            user: None,
            default_bbox: None,
        }));
    };
    let username = users::username(&pool, session.user_id).await?;
    let colors = scoring::player_colors(&pool).await?;
    Ok(Json(Me {
        default_bbox: territory_extent(&pool, session.user_id).await?,
        user: Some(Player {
            color: scoring::color_of(&colors, &username),
            strava_connected: users::strava_connected(&pool, session.user_id).await?,
            username,
            csrf_token: session.csrf_token,
        }),
    }))
}

/// Bounding box of a player's current cells and passes, if they hold any.
async fn territory_extent(pool: &PgPool, user_id: i64) -> ApiResult<Option<[f64; 4]>> {
    let cells = sqlx::query!(
        r#"SELECT ST_XMin(e) AS "x0", ST_YMin(e) AS "y0", ST_XMax(e) AS "x1", ST_YMax(e) AS "y1"
           FROM (SELECT ST_Extent(centroid) AS e FROM game_territorycell WHERE owner_id = $1) t"#,
        user_id
    )
    .fetch_one(pool)
    .await?;
    let pois = sqlx::query!(
        r#"SELECT ST_XMin(e) AS "x0", ST_YMin(e) AS "y0", ST_XMax(e) AS "x1", ST_YMax(e) AS "y1"
           FROM (SELECT ST_Extent(location) AS e FROM game_poi WHERE owner_id = $1) t"#,
        user_id
    )
    .fetch_one(pool)
    .await?;
    let boxes: Vec<[f64; 4]> = [
        (cells.x0, cells.y0, cells.x1, cells.y1),
        (pois.x0, pois.y0, pois.x1, pois.y1),
    ]
    .into_iter()
    .filter_map(|(a, b, c, d)| Some([a?, b?, c?, d?]))
    .collect();
    Ok(boxes.into_iter().reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }))
}

pub(super) async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Session,
) -> ApiResult<(CookieJar, StatusCode)> {
    Ok((
        state.sessions.end(jar, &session).await?,
        StatusCode::NO_CONTENT,
    ))
}
#[derive(Serialize)]
pub(super) struct Profile {
    username: String,
    color: String,
    member_since: DateTime<Utc>,
    score: scoring::Score,
    territory_pct: i64,
    poi_pct: i64,
    rank: Option<usize>,
    player_count: usize,
    detail: scoring::CaptureDetail,
    strava_connected: bool,
}

pub(super) async fn profile(
    State(pool): State<PgPool>,
    session: Session,
) -> ApiResult<Json<Profile>> {
    let now = Utc::now();
    let username = users::username(&pool, session.user_id).await?;
    let board = scoring::leaderboard(&pool, now, false).await?;
    let score = scoring::player_score(&pool, session.user_id, now).await?;
    let (territory_pct, poi_pct) = scoring::score_split_pct(&score);
    let colors = scoring::player_colors(&pool).await?;
    Ok(Json(Profile {
        color: scoring::color_of(&colors, &username),
        member_since: users::joined(&pool, session.user_id).await?,
        rank: board
            .iter()
            .position(|r| r.username == username)
            .map(|i| i + 1),
        player_count: board.len(),
        detail: scoring::player_capture_detail(&pool, session.user_id, now).await?,
        strava_connected: users::strava_connected(&pool, session.user_id).await?,
        username,
        score,
        territory_pct,
        poi_pct,
    }))
}

/// Delete the player's account: revoke Strava access, wipe their data, let the others reclaim.
pub(super) async fn delete_account(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Session,
) -> ApiResult<(CookieJar, StatusCode)> {
    state.services.delete_account(session.user_id).await?;
    let jar = state.sessions.end(jar, &session).await?;
    Ok((jar, StatusCode::NO_CONTENT))
}
