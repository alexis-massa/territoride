//! What the app does with Strava for a player: keep their token fresh, pull their recent activities, react to
//! webhook events, and say goodbye when they delete their account.

use chrono::{DateTime, Utc};
use pi_common::AppError;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;

use crate::capture;
use crate::crypto::TokenCipher;
use crate::import::{ImportCounts, import_activities};
use crate::strava::{Strava, StravaError};
use crate::users;

/// A Strava webhook event, as far as we read it; absent or odd fields default so the handler can ignore them.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WebhookEvent {
    pub owner_id: Option<i64>,
    pub object_type: String,
    pub object_id: Option<i64>,
    pub aspect_type: String,
    /// Strava sends `"authorized": "false"` as a string.
    pub updates: HashMap<String, Value>,
}

/// Why a sync did not happen.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Connect with Strava first.")]
    NotConnected,
    #[error(transparent)]
    Strava(#[from] StravaError),
    #[error(transparent)]
    App(#[from] AppError),
}

impl From<sqlx::Error> for SyncError {
    fn from(e: sqlx::Error) -> Self {
        Self::App(e.into())
    }
}

/// Everything the Strava-facing code needs.
#[derive(Clone)]
pub struct Services {
    pub pool: PgPool,
    pub cipher: TokenCipher,
    pub strava: Strava,
}

impl Services {
    /// The player's access token, refreshed (and stored) first if it has expired.
    pub async fn valid_token(&self, user_id: i64, now: DateTime<Utc>) -> Result<String, SyncError> {
        let stored = users::tokens(&self.pool, &self.cipher, user_id).await?;
        if stored.refresh.is_empty() {
            return Err(SyncError::NotConnected);
        }
        if stored.expires_at.is_some_and(|at| at > now) {
            return Ok(stored.access);
        }
        let fresh = self.strava.refresh(&stored.refresh).await?;
        users::store_tokens(&self.pool, &self.cipher, user_id, &fresh).await?;
        Ok(fresh.access_token)
    }

    /// Pull the player's recent activities and capture what they hold.
    pub async fn sync_player(
        &self,
        user_id: i64,
        now: DateTime<Utc>,
    ) -> Result<ImportCounts, SyncError> {
        let token = self.valid_token(user_id, now).await?;
        let activities = self.strava.recent_activities(&token, now).await?;
        Ok(import_activities(&self.pool, user_id, &activities).await?)
    }

    /// Process one Strava webhook event: a new or changed activity, a deleted one, or a revoked authorisation.
    pub async fn handle_event(
        &self,
        event: &WebhookEvent,
        now: DateTime<Utc>,
    ) -> Result<(), SyncError> {
        let Some(owner) = event.owner_id else {
            return Ok(());
        };
        let Some(user_id) = users::id_by_athlete(&self.pool, owner).await? else {
            return Ok(());
        };
        if event
            .updates
            .get("authorized")
            .is_some_and(|v| v == "false")
        {
            users::clear_tokens(&self.pool, user_id).await?;
            return Ok(());
        }
        if event.object_type != "activity" {
            return Ok(());
        }
        let Some(object_id) = event.object_id else {
            return Ok(());
        };
        match event.aspect_type.as_str() {
            "delete" => {
                let activity = sqlx::query_scalar!(
                    "SELECT id FROM game_activity WHERE strava_activity_id = $1",
                    object_id
                )
                .fetch_optional(&self.pool)
                .await?;
                if let Some(id) = activity {
                    capture::release_activity(&self.pool, id).await?;
                }
            }
            "create" | "update" => {
                let token = self.valid_token(user_id, now).await?;
                let activity = self.strava.activity(&token, object_id).await?;
                import_activities(&self.pool, user_id, &[activity]).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Delete a player: revoke Strava access (best effort), wipe their data, let the others reclaim.
    pub async fn delete_account(&self, user_id: i64) -> Result<(), AppError> {
        if let Ok(tokens) = users::tokens(&self.pool, &self.cipher, user_id).await
            && !tokens.access.is_empty()
            && let Err(e) = self.strava.deauthorize(&tokens.access).await
        {
            tracing::warn!(error = %e, "could not revoke the Strava access");
        }
        capture::delete_user(&self.pool, user_id).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn create_event_is_read() {
        let e: WebhookEvent = serde_json::from_value(json!({
            "aspect_type": "create", "event_time": 1549560669, "object_id": 1360128428,
            "object_type": "activity", "owner_id": 134815, "subscription_id": 120475,
            "updates": {}
        }))
        .unwrap();
        assert_eq!(e.owner_id, Some(134815));
        assert_eq!(e.object_id, Some(1360128428));
        assert_eq!(
            (e.object_type.as_str(), e.aspect_type.as_str()),
            ("activity", "create")
        );
        assert!(e.updates.is_empty());
    }

    #[test]
    fn deauthorisation_event_is_read() {
        let e: WebhookEvent = serde_json::from_value(json!({
            "aspect_type": "update", "object_id": 134815, "object_type": "athlete",
            "owner_id": 134815, "updates": {"authorized": "false"}
        }))
        .unwrap();
        assert!(e.updates.get("authorized").is_some_and(|v| v == "false"));
    }
}
