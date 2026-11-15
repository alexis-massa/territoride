//! HTTP routes: the JSON API under `/api`, the Strava webhook, and the embedded Svelte app for the rest.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, FromRef, Multipart, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Utc};
use pi_common::auth::{Session, SessionOptions, Sessions};
use pi_common::cli::Context;
use pi_common::{ApiResult, AppError, BoxError, static_files};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower_http::compression::CompressionLayer;

use crate::capture;
use crate::decay::DECAY_THRESHOLD_DAYS;
use crate::settings::Settings;
use crate::strava::Strava;
use crate::sync::{Services, SyncError};
use crate::{geojson, gpx_file, scheduler, scoring, users};

/// The Svelte build (`web/dist`), embedded at compile time; read from disk in debug builds.
#[derive(RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Assets;

const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const OAUTH_STATE_COOKIE: &str = "strava_oauth_state";
/// Where Strava sends the player back; also the path registered with the webhook, kept from the Python version
/// so the existing subscription keeps working.
const WEBHOOK_PATH: &str = "/accounts/strava/webhook/";
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Shared state of the handlers.
#[derive(Clone, FromRef)]
pub struct AppState {
    pool: PgPool,
    sessions: Sessions,
    services: Services,
    web: Arc<WebConfig>,
}

struct WebConfig {
    origin: String,
    cookie_secure: bool,
    webhook_verify_token: String,
}

/// The Content-Security-Policy: the map loads its style, tiles, glyphs and sprites from OpenFreeMap and runs
/// its renderer in a blob worker.
pub const CSP: &str = "default-src 'self'; img-src 'self' data: blob: https://tiles.openfreemap.org; \
    style-src 'self'; style-src-attr 'unsafe-inline'; script-src 'self'; worker-src 'self' blob:; \
    child-src blob:; connect-src 'self' https://tiles.openfreemap.org; frame-ancestors 'none'; \
    base-uri 'none'; form-action 'self'";

/// Build the router for `serve`: settings from the environment, the daily release task started.
pub async fn build(ctx: Context) -> Result<Router, BoxError> {
    let settings = Settings::from_env()?;
    let strava = Strava::new(&settings.strava_client_id, &settings.strava_client_secret);
    if let Some(hour) = settings.decay_hour_utc {
        scheduler::spawn_daily_release(ctx.pool.clone(), hour);
    }
    build_with(ctx, &settings, strava).await
}

/// The router with its collaborators given (tests point `strava` at a stand-in).
pub async fn build_with(
    ctx: Context,
    settings: &Settings,
    strava: Strava,
) -> Result<Router, BoxError> {
    let sessions = Sessions::new(
        ctx.pool.clone(),
        SessionOptions {
            table: "app_sessions",
            cookie_name: "territoride_session",
            days: ctx.config.session_days,
            secure: ctx.config.cookie_secure,
        },
    );
    sessions.purge_expired().await?;
    let scheme = if ctx.config.cookie_secure {
        "https"
    } else {
        "http"
    };
    let state = AppState {
        services: Services {
            pool: ctx.pool.clone(),
            cipher: settings.cipher.clone(),
            strava,
        },
        pool: ctx.pool,
        sessions,
        web: Arc::new(WebConfig {
            origin: format!("{scheme}://{}", ctx.config.app_domain),
            cookie_secure: ctx.config.cookie_secure,
            webhook_verify_token: settings.webhook_verify_token.clone(),
        }),
    };
    let map_data = Router::new()
        .route("/api/territory.geojson", get(geojson::territory))
        .route("/api/routes.geojson", get(geojson::routes))
        .route("/api/pois.geojson", get(geojson::pois))
        .layer(CompressionLayer::new());
    Ok(Router::new()
        .merge(map_data)
        .route("/api/me", get(me))
        .route("/api/logout", post(logout))
        .route("/api/leaderboard", get(leaderboard))
        .route("/api/profile", get(profile))
        .route("/api/rules", get(rules))
        .route("/api/changelog", get(changelog))
        .route("/api/strava/authorize", get(strava_authorize))
        .route("/api/strava/callback", get(strava_callback))
        .route("/api/strava/sync", post(strava_sync))
        .route(WEBHOOK_PATH, get(webhook_challenge).post(webhook_event))
        .route(
            "/api/import-activity",
            post(import_activity).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route("/api/account/delete", post(delete_account))
        .with_state(state)
        .merge(static_files::router::<Assets, ()>()))
}

/// The logged-in player, if any, and where the map should open.
#[derive(Serialize)]
struct Me {
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

async fn me(State(pool): State<PgPool>, session: Option<Session>) -> ApiResult<Json<Me>> {
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

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Session,
) -> ApiResult<(CookieJar, StatusCode)> {
    Ok((
        state.sessions.end(jar, &session).await?,
        StatusCode::NO_CONTENT,
    ))
}

async fn leaderboard(State(pool): State<PgPool>) -> ApiResult<Json<Vec<scoring::LeaderboardRow>>> {
    Ok(Json(scoring::leaderboard(&pool, Utc::now(), true).await?))
}

#[derive(Serialize)]
struct Profile {
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

async fn profile(State(pool): State<PgPool>, session: Session) -> ApiResult<Json<Profile>> {
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
async fn rules() -> Json<Value> {
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

async fn changelog() -> Json<Value> {
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(CHANGELOG));
    Json(json!({ "html": html }))
}

fn oauth_redirect_uri(web: &WebConfig) -> String {
    format!("{}/api/strava/callback", web.origin)
}

/// Start the Strava handshake: remember a random state (in a cookie that survives the trip through Strava) and
/// send the player to Strava.
async fn strava_authorize(State(state): State<AppState>, jar: CookieJar) -> (CookieJar, Redirect) {
    let nonce = uuid_like();
    let cookie = Cookie::build((OAUTH_STATE_COOKIE, nonce.clone()))
        .http_only(true)
        .secure(state.web.cookie_secure)
        // Lax: the cookie must come back with Strava's redirect, a cross-site navigation.
        .same_site(SameSite::Lax)
        .path("/api/strava")
        .build();
    let url = state
        .services
        .strava
        .authorize_url(&oauth_redirect_uri(&state.web), &nonce);
    (jar.add(cookie), Redirect::to(&url))
}

fn uuid_like() -> String {
    let mut bytes = [0u8; 24];
    rand::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Deserialize)]
struct CallbackQuery {
    error: Option<String>,
    state: Option<String>,
    code: Option<String>,
}

/// Where the popup ends up: a page of the app that tells the opener how it went and closes itself.
fn oauth_done(status: &str, message: &str) -> Redirect {
    let query = reqwest::Url::parse_with_params(
        "http://x/oauth/complete",
        [("status", status), ("message", message)],
    )
    .map(|u| u.query().unwrap_or_default().to_owned())
    .unwrap_or_default();
    Redirect::to(&format!("/oauth/complete?{query}"))
}

/// Strava's redirect: exchange the code, find or create the player, log them in.
async fn strava_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> ApiResult<(CookieJar, Redirect)> {
    let expected = jar.get(OAUTH_STATE_COOKIE).map(|c| c.value().to_owned());
    let jar = jar.remove(
        Cookie::build(OAUTH_STATE_COOKIE)
            .path("/api/strava")
            .build(),
    );
    if let Some(error) = q.error {
        return Ok((
            jar,
            oauth_done("error", &format!("Strava denied access: {error}")),
        ));
    }
    if q.state.is_none() || q.state != expected {
        return Ok((jar, oauth_done("error", "Invalid OAuth state")));
    }
    let Some(code) = q.code else {
        return Ok((jar, oauth_done("error", "Missing authorization code")));
    };
    let services = &state.services;
    let tokens = match services.strava.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "Strava code exchange failed");
            return Ok((jar, oauth_done("error", "Could not reach Strava")));
        }
    };
    let Some(athlete) = tokens.athlete.clone() else {
        return Ok((jar, oauth_done("error", "Strava sent no athlete")));
    };
    let user_id =
        users::login_from_strava(&services.pool, &services.cipher, &athlete, &tokens).await?;
    let (jar, _) = state.sessions.start(jar, user_id).await?;
    Ok((jar, oauth_done("success", "Connected!")))
}

/// What a sync or an upload tells the player.
#[derive(Serialize)]
struct Outcome {
    ok: bool,
    message: String,
}

impl Outcome {
    fn ok(message: impl Into<String>) -> Json<Self> {
        Json(Self {
            ok: true,
            message: message.into(),
        })
    }

    fn failed(message: impl Into<String>) -> Json<Self> {
        Json(Self {
            ok: false,
            message: message.into(),
        })
    }
}

/// Pull the player's recent Strava activities.
async fn strava_sync(State(state): State<AppState>, session: Session) -> ApiResult<Json<Outcome>> {
    match state
        .services
        .sync_player(session.user_id, Utc::now())
        .await
    {
        Ok(c) if c.activities > 0 => Ok(Outcome::ok(format!(
            "Synced {} activities: captured {} cells, {} passes.",
            c.activities, c.cells, c.pois
        ))),
        Ok(_) => Ok(Outcome::ok("No new activities to sync.")),
        Err(SyncError::App(e)) => Err(e),
        Err(e) => Ok(Outcome::failed(e.to_string())),
    }
}

#[derive(Deserialize)]
struct Challenge {
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

/// Strava checks a new subscription by asking us to echo a challenge.
async fn webhook_challenge(State(state): State<AppState>, Query(q): Query<Challenge>) -> Response {
    if q.verify_token.as_deref() != Some(state.web.webhook_verify_token.as_str()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(json!({ "hub.challenge": q.challenge.unwrap_or_default() })).into_response()
}

/// An event from Strava: answered at once (Strava wants a reply within two seconds), handled in the background.
async fn webhook_event(State(state): State<AppState>, Json(event): Json<Value>) -> StatusCode {
    let services = state.services.clone();
    tokio::spawn(async move {
        if let Err(e) = services.handle_event(&event, Utc::now()).await {
            tracing::warn!(error = %e, %event, "Strava event not handled");
        }
    });
    StatusCode::OK
}

/// Import an uploaded GPX file as an activity of the player.
async fn import_activity(
    State(pool): State<PgPool>,
    session: Session,
    mut form: Multipart,
) -> ApiResult<Json<Outcome>> {
    let mut file = None;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("gpx_file") {
            let name = field.file_name().unwrap_or_default().to_owned();
            file = Some((
                name,
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?,
            ));
        }
    }
    let Some((file_name, bytes)) = file.filter(|(_, b)| !b.is_empty()) else {
        return Ok(Outcome::failed("No file selected."));
    };
    let Ok(track) = gpx_file::parse(bytes.as_ref()) else {
        return Ok(Outcome::failed("Couldn't read that file as GPX."));
    };
    if track.points.len() < 2 {
        return Ok(Outcome::failed("GPX file has fewer than 2 track points."));
    }
    let Some(recorded_at) = track.started_at else {
        return Ok(Outcome::failed("GPX file is missing activity date/time."));
    };
    let name = track.name.unwrap_or_else(|| {
        if file_name.is_empty() {
            "Untitled activity".into()
        } else {
            file_name
        }
    });
    let (cells, pois) =
        gpx_file::store(&pool, session.user_id, &name, recorded_at, &track.points).await?;
    Ok(Outcome::ok(format!(
        "Imported {name:?}: captured {cells} cells, {pois} passes."
    )))
}

/// Delete the player's account: revoke Strava access, wipe their data, let the others reclaim.
async fn delete_account(
    State(state): State<AppState>,
    jar: CookieJar,
    session: Session,
) -> ApiResult<(CookieJar, StatusCode)> {
    state.services.delete_account(session.user_id).await?;
    let jar = state.sessions.end(jar, &session).await?;
    Ok((jar, StatusCode::NO_CONTENT))
}
