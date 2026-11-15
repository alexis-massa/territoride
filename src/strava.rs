//! The Strava API: OAuth, the activity list, deauthorisation and webhook subscriptions.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{RequestBuilder, Response, StatusCode, Url};
use serde::Deserialize;
use serde_json::Value;

use crate::decay::DECAY_THRESHOLD_DAYS;
use crate::import::StravaActivity;

const PAGE_SIZE: usize = 30;

/// What went wrong talking to Strava.
#[derive(Debug, thiserror::Error)]
pub enum StravaError {
    /// Strava answered with an error status.
    #[error("Strava returned an error: {0}")]
    Status(String),
    /// No usable answer (network, timeout, malformed body).
    #[error("couldn't reach Strava")]
    Unreachable(#[source] reqwest::Error),
}

impl From<reqwest::Error> for StravaError {
    fn from(e: reqwest::Error) -> Self {
        Self::Unreachable(e)
    }
}

/// Strava's answer to a token request; the athlete is only present for the first (code) exchange.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub athlete: Option<Athlete>,
}

impl TokenResponse {
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.expires_at, 0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Athlete {
    pub id: i64,
    pub username: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
}

/// A client with the app's Strava credentials. `base` is overridable so tests can point it at a stand-in.
#[derive(Clone)]
pub struct Strava {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    base: String,
}

impl Strava {
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self::with_base(client_id, client_secret, "https://www.strava.com")
    }

    pub fn with_base(client_id: &str, client_secret: &str, base: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("territoride/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the HTTP client builds");
        Self {
            http,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            base: base.into(),
        }
    }

    /// Where to send the player to approve access.
    pub fn authorize_url(&self, redirect_uri: &str, state: &str) -> String {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("approval_prompt", "auto"),
            ("scope", "read,activity:read"),
            ("state", state),
        ];
        Url::parse_with_params(&format!("{}/oauth/authorize", self.base), params)
            .expect("the base URL is valid")
            .into()
    }

    pub async fn exchange_code(&self, code: &str) -> Result<TokenResponse, StravaError> {
        self.token_request(&[("code", code), ("grant_type", "authorization_code")])
            .await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenResponse, StravaError> {
        self.token_request(&[
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .await
    }

    async fn token_request(&self, extra: &[(&str, &str)]) -> Result<TokenResponse, StravaError> {
        let mut form = vec![
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ];
        form.extend_from_slice(extra);
        let response = checked(
            self.http
                .post(format!("{}/oauth/token", self.base))
                .form(&form),
        )
        .await?;
        Ok(response.json().await?)
    }

    /// Every activity of the decay window (older ones would score nothing).
    pub async fn recent_activities(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<StravaActivity>, StravaError> {
        let after = (now - chrono::Duration::days(DECAY_THRESHOLD_DAYS))
            .timestamp()
            .to_string();
        let mut all = Vec::new();
        for page in 1.. {
            let request = self
                .http
                .get(format!("{}/api/v3/athlete/activities", self.base))
                .bearer_auth(token)
                .query(&[
                    ("after", after.as_str()),
                    ("per_page", &PAGE_SIZE.to_string()),
                    ("page", &page.to_string()),
                ]);
            let batch: Vec<StravaActivity> = checked(request).await?.json().await?;
            let last = batch.len() < PAGE_SIZE;
            all.extend(batch);
            if last {
                break;
            }
        }
        Ok(all)
    }

    pub async fn activity(&self, token: &str, id: i64) -> Result<StravaActivity, StravaError> {
        let request = self
            .http
            .get(format!("{}/api/v3/activities/{id}", self.base))
            .bearer_auth(token);
        Ok(checked(request).await?.json().await?)
    }

    /// Revoke the app's access to a player's Strava account.
    pub async fn deauthorize(&self, token: &str) -> Result<(), StravaError> {
        checked(
            self.http
                .post(format!("{}/oauth/deauthorize", self.base))
                .form(&[("access_token", token)]),
        )
        .await?;
        Ok(())
    }

    fn credentials(&self) -> [(&str, &str); 2] {
        [
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ]
    }

    fn subscriptions_url(&self) -> String {
        format!("{}/api/v3/push_subscriptions", self.base)
    }

    pub async fn create_subscription(
        &self,
        callback_url: &str,
        verify_token: &str,
    ) -> Result<Value, StravaError> {
        let mut form = self.credentials().to_vec();
        form.extend([
            ("callback_url", callback_url),
            ("verify_token", verify_token),
        ]);
        Ok(
            checked(self.http.post(self.subscriptions_url()).form(&form))
                .await?
                .json()
                .await?,
        )
    }

    pub async fn subscriptions(&self) -> Result<Vec<Value>, StravaError> {
        Ok(checked(
            self.http
                .get(self.subscriptions_url())
                .query(&self.credentials()),
        )
        .await?
        .json()
        .await?)
    }

    pub async fn delete_subscription(&self, id: i64) -> Result<(), StravaError> {
        checked(
            self.http
                .delete(format!("{}/{id}", self.subscriptions_url()))
                .query(&self.credentials()),
        )
        .await?;
        Ok(())
    }
}

/// Send a request; a non-success status becomes an error carrying the start of Strava's own message.
async fn checked(request: RequestBuilder) -> Result<Response, StravaError> {
    let response = request.send().await?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status: StatusCode = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(StravaError::Status(format!(
        "{status}: {}",
        body.chars().take(200).collect::<String>()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_authorize_url_carries_every_parameter() {
        let url = Strava::new("42", "secret").authorize_url("https://x.test/cb", "st&te");
        let url = Url::parse(&url).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(url.path(), "/oauth/authorize");
        assert_eq!(params["client_id"], "42");
        assert_eq!(params["redirect_uri"], "https://x.test/cb");
        assert_eq!(params["scope"], "read,activity:read");
        assert_eq!(params["state"], "st&te");
        assert!(!url.as_str().contains("secret"));
    }
}
