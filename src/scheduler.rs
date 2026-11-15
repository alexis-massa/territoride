//! The daily release of expired ownership, run inside the server (the `release-expired` command does the
//! same by hand).

use chrono::{DateTime, Duration, NaiveTime, Utc};
use sqlx::PgPool;

use crate::capture::release_expired;

/// The next time after `now` that the clock reads `hour`:00 UTC.
pub fn next_run(now: DateTime<Utc>, hour: u32) -> DateTime<Utc> {
    let today = now
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(hour, 0, 0).unwrap_or_default())
        .and_utc();
    if today > now {
        today
    } else {
        today + Duration::days(1)
    }
}

/// Release what has expired every day at `hour`:00 UTC, for as long as the server runs.
pub fn spawn_daily_release(pool: PgPool, hour: u32) {
    tokio::spawn(async move {
        loop {
            let now = Utc::now();
            let wait = (next_run(now, hour) - now).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;
            match release_expired(&pool, Utc::now()).await {
                Ok(n) => tracing::info!(released = n, "released expired cells and passes"),
                Err(e) => tracing::error!(error = %e, "release of expired ownership failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn runs_today_if_the_hour_is_ahead_else_tomorrow() {
        assert_eq!(
            next_run(at("2026-11-15T03:59:59Z"), 4),
            at("2026-11-15T04:00:00Z")
        );
        assert_eq!(
            next_run(at("2026-11-15T04:00:00Z"), 4),
            at("2026-11-16T04:00:00Z")
        );
        assert_eq!(
            next_run(at("2026-11-15T23:00:00Z"), 0),
            at("2026-11-16T00:00:00Z")
        );
    }
}
