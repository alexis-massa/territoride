//! Strava: the OAuth handshake, the manual sync and the webhook.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use pi_common::ApiResult;
use pi_common::auth::Session;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{AppState, Outcome, WebConfig};
use crate::sync::SyncError;
use crate::users;

const OAUTH_STATE_COOKIE: &str = "strava_oauth_state";
/// Where Strava sends the player back; also the path registered with the webhook, kept from the Python version
/// so the existing subscription keeps working.
pub(super) const WEBHOOK_PATH: &str = "/accounts/strava/webhook/";

fn oauth_redirect_uri(web: &WebConfig) -> String {
    format!("{}/api/strava/callback", web.origin)
}

/// Start the Strava handshake: remember a random state (in a cookie that survives the trip through Strava) and
/// send the player to Strava.
pub(super) async fn strava_authorize(
    State(state): State<AppState>,
    jar: CookieJar,
) -> (CookieJar, Redirect) {
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
pub(super) struct CallbackQuery {
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
pub(super) async fn strava_callback(
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
/// Pull the player's recent Strava activities.
pub(super) async fn strava_sync(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<Outcome>> {
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
pub(super) struct Challenge {
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
}

/// Strava checks a new subscription by asking us to echo a challenge.
pub(super) async fn webhook_challenge(
    State(state): State<AppState>,
    Query(q): Query<Challenge>,
) -> Response {
    if q.verify_token.as_deref() != Some(state.web.webhook_verify_token.as_str()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(json!({ "hub.challenge": q.challenge.unwrap_or_default() })).into_response()
}

/// An event from Strava: answered at once (Strava wants a reply within two seconds), handled in the background.
pub(super) async fn webhook_event(
    State(state): State<AppState>,
    Json(event): Json<Value>,
) -> StatusCode {
    let services = state.services.clone();
    tokio::spawn(async move {
        if let Err(e) = services.handle_event(&event, Utc::now()).await {
            tracing::warn!(error = %e, %event, "Strava event not handled");
        }
    });
    StatusCode::OK
}
