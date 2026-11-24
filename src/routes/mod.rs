//! HTTP routes: the JSON API under `/api`, the Strava webhook, and the embedded Svelte app for the rest.
//!
//! | Route | Module |
//! |---|---|
//! | `/api/me`, `/api/logout`, `/api/profile`, `/api/account/delete` | [`account`] |
//! | `/api/leaderboard`, `/api/rules`, `/api/changelog` | [`game`] |
//! | `/api/strava/*`, the webhook | [`strava`] |
//! | `/api/import-activity` | [`import`] |
//! | `/api/*.geojson` (map data) | [`crate::geojson`] |

mod account;
mod game;
mod import;
mod strava;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, FromRef};
use axum::routing::{get, post};
use axum::{Json, Router};
use pi_common::auth::{SessionOptions, Sessions};
use pi_common::cli::Context;
use pi_common::{BoxError, static_files};
use rust_embed::RustEmbed;
use serde::Serialize;
use sqlx::PgPool;
use tower_http::compression::CompressionLayer;

use crate::settings::Settings;
use crate::strava::Strava;
use crate::sync::Services;
use crate::{geojson, scheduler};

/// The Svelte build (`web/dist`), embedded at compile time; read from disk in debug builds.
#[derive(RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Assets;

/// Shared state of the handlers.
#[derive(Clone, FromRef)]
pub struct AppState {
    sessions: Sessions,
    services: Services,
    web: Arc<WebConfig>,
}

/// The pool lives in `services` only; handlers that just need it extract `State<PgPool>`.
impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.services.pool.clone()
    }
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
        .route("/api/me", get(account::me))
        .route("/api/logout", post(account::logout))
        .route("/api/leaderboard", get(game::leaderboard))
        .route("/api/profile", get(account::profile))
        .route("/api/rules", get(game::rules))
        .route("/api/changelog", get(game::changelog))
        .route("/api/strava/authorize", get(strava::strava_authorize))
        .route("/api/strava/callback", get(strava::strava_callback))
        .route("/api/strava/sync", post(strava::strava_sync))
        .route(
            strava::WEBHOOK_PATH,
            get(strava::webhook_challenge).post(strava::webhook_event),
        )
        .route(
            "/api/import-activity",
            post(import::import_activity).layer(DefaultBodyLimit::max(import::MAX_UPLOAD_BYTES)),
        )
        .route("/api/account/delete", post(account::delete_account))
        .with_state(state)
        .merge(static_files::router::<Assets, ()>()))
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
