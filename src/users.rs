//! Players: created and found through Strava; their tokens are stored encrypted.

use chrono::{DateTime, Utc};
use pi_common::ApiResult;
use sqlx::PgPool;

use crate::crypto::TokenCipher;
use crate::strava::{Athlete, TokenResponse};

pub async fn username(pool: &PgPool, user_id: i64) -> ApiResult<String> {
    Ok(
        sqlx::query_scalar!("SELECT username FROM accounts_user WHERE id = $1", user_id)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn id_by_username(pool: &PgPool, username: &str) -> ApiResult<i64> {
    Ok(
        sqlx::query_scalar!("SELECT id FROM accounts_user WHERE username = $1", username)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn id_by_athlete(pool: &PgPool, athlete_id: i64) -> ApiResult<Option<i64>> {
    Ok(sqlx::query_scalar!(
        "SELECT id FROM accounts_user WHERE athlete_id = $1",
        athlete_id
    )
    .fetch_optional(pool)
    .await?)
}

/// The player behind a Strava athlete, created on first login (named after their Strava username, or
/// `athlete_<id>`; a name that is taken gets the id appended), with the tokens just issued.
pub async fn login_from_strava(
    pool: &PgPool,
    cipher: &TokenCipher,
    athlete: &Athlete,
    tokens: &TokenResponse,
) -> ApiResult<i64> {
    let wanted = athlete
        .username
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("athlete_{}", athlete.id));
    // Django marks "no password" with a leading "!"; players never log in with one.
    let unusable = format!("!{}", random_hex());
    let id = sqlx::query_scalar!(
        r#"INSERT INTO accounts_user (password, is_superuser, username, first_name, last_name, email, is_staff,
               is_active, date_joined, athlete_id, strava_access_token_encrypted, strava_refresh_token_encrypted)
           VALUES ($1, false, CASE WHEN EXISTS (SELECT 1 FROM accounts_user WHERE username = $2) THEN $2 || '_' || $3::bigint::text ELSE $2 END,
                   '', '', '', false, true, now(), $3::bigint, '', '')
           ON CONFLICT (athlete_id) DO UPDATE SET athlete_id = EXCLUDED.athlete_id
           RETURNING id AS "id!""#,
        unusable,
        wanted,
        athlete.id
    )
    .fetch_one(pool)
    .await?;
    sqlx::query!(
        "UPDATE accounts_user SET first_name = $2, last_name = $3 WHERE id = $1",
        id,
        athlete.firstname.clone().unwrap_or_default(),
        athlete.lastname.clone().unwrap_or_default()
    )
    .execute(pool)
    .await?;
    store_tokens(pool, cipher, id, tokens).await?;
    Ok(id)
}

pub async fn store_tokens(
    pool: &PgPool,
    cipher: &TokenCipher,
    user_id: i64,
    tokens: &TokenResponse,
) -> ApiResult<()> {
    sqlx::query!(
        "UPDATE accounts_user SET strava_access_token_encrypted = $2, strava_refresh_token_encrypted = $3,
                strava_token_expires_at = $4 WHERE id = $1",
        user_id,
        cipher.encrypt(&tokens.access_token),
        cipher.encrypt(&tokens.refresh_token),
        tokens.expires_at()
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_tokens(pool: &PgPool, user_id: i64) -> ApiResult<()> {
    sqlx::query!(
        "UPDATE accounts_user SET strava_access_token_encrypted = '', strava_refresh_token_encrypted = '',
                strava_token_expires_at = NULL WHERE id = $1",
        user_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A player's stored Strava tokens, decrypted.
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn tokens(pool: &PgPool, cipher: &TokenCipher, user_id: i64) -> ApiResult<Tokens> {
    let row = sqlx::query!(
        "SELECT strava_access_token_encrypted AS access, strava_refresh_token_encrypted AS refresh,
                strava_token_expires_at AS expires_at FROM accounts_user WHERE id = $1",
        user_id
    )
    .fetch_one(pool)
    .await?;
    Ok(Tokens {
        access: cipher.decrypt(&row.access)?,
        refresh: cipher.decrypt(&row.refresh)?,
        expires_at: row.expires_at,
    })
}

/// Players who have connected Strava, as `(id, username)`.
pub async fn connected(pool: &PgPool) -> ApiResult<Vec<(i64, String)>> {
    let rows = sqlx::query!("SELECT id, username FROM accounts_user WHERE strava_refresh_token_encrypted <> '' ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.username)).collect())
}

/// Everyone, as `(id, username, athlete id, connected)`, for the admin commands.
pub async fn all(pool: &PgPool) -> ApiResult<Vec<(i64, String, Option<i64>, bool)>> {
    let rows = sqlx::query!(
        r#"SELECT id, username, athlete_id, strava_refresh_token_encrypted <> '' AS "connected!" FROM accounts_user ORDER BY id"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, r.username, r.athlete_id, r.connected))
        .collect())
}

fn random_hex() -> String {
    let mut bytes = [0u8; 20];
    rand::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether the player has connected Strava (a refresh token is stored).
pub async fn strava_connected(pool: &PgPool, user_id: i64) -> ApiResult<bool> {
    Ok(sqlx::query_scalar!(
        r#"SELECT strava_refresh_token_encrypted <> '' AS "connected!" FROM accounts_user WHERE id = $1"#,
        user_id
    )
    .fetch_one(pool)
    .await?)
}

/// When the player joined.
pub async fn joined(pool: &PgPool, user_id: i64) -> ApiResult<DateTime<Utc>> {
    Ok(sqlx::query_scalar!(
        "SELECT date_joined FROM accounts_user WHERE id = $1",
        user_id
    )
    .fetch_one(pool)
    .await?)
}
