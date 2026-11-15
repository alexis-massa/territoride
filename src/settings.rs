//! The app's own settings (the shared ones are in `pi_common::config`), from the environment.

use pi_common::config::{ConfigError, env_or, env_required};

use crate::crypto::TokenCipher;

/// Strava credentials, the token key and the daily release hour.
#[derive(Clone)]
pub struct Settings {
    pub strava_client_id: String,
    pub strava_client_secret: String,
    pub webhook_verify_token: String,
    pub cipher: TokenCipher,
    /// Hour (UTC) of the daily release of expired ownership; `None` (set `-1`) turns the in-process task off.
    pub decay_hour_utc: Option<u32>,
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        let hour: i32 = env_or("DECAY_HOUR_UTC", 4)?;
        Ok(Self {
            strava_client_id: env_or("STRAVA_CLIENT_ID", String::new())?,
            strava_client_secret: env_or("STRAVA_CLIENT_SECRET", String::new())?,
            webhook_verify_token: env_or("STRAVA_WEBHOOK_VERIFY_TOKEN", String::new())?,
            cipher: TokenCipher::new(&env_required("TOKEN_ENCRYPTION_KEY")?).map_err(|reason| {
                ConfigError::Invalid {
                    name: "TOKEN_ENCRYPTION_KEY",
                    reason,
                }
            })?,
            decay_hour_utc: u32::try_from(hour).ok().filter(|h| *h < 24),
        })
    }
}
