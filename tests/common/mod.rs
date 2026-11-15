//! Shared helpers for the integration tests: a fresh migrated database per test.
//! Run with `just test` (it starts the local Postgres and sets `TEST_DATABASE_URL`).
#![allow(dead_code)] // each test binary uses a different subset

use std::sync::atomic::{AtomicU32, Ordering};

use sqlx::{AssertSqlSafe, PgPool};

pub fn test_url() -> String {
    std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL (run `just test`)")
}

/// A database of its own, migrated.
pub async fn fresh_db() -> PgPool {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let name = format!(
        "td_test_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let base = test_url();
    let admin = pi_common::db::connect(&base).await.unwrap();
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .unwrap();
    let (server, _) = base.rsplit_once('/').unwrap();
    let pool = pi_common::db::connect(&format!("{server}/{name}"))
        .await
        .unwrap();
    pi_common::db::migrate(&pool, territoride::migrator())
        .await
        .unwrap();
    pool
}

/// A player, by name.
pub async fn add_user(pool: &PgPool, username: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO accounts_user (password, is_superuser, username, first_name, last_name, email, is_staff,
            is_active, date_joined, strava_access_token_encrypted, strava_refresh_token_encrypted)
         VALUES ('', false, $1, '', '', '', false, true, now(), '', '') RETURNING id",
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap()
}
