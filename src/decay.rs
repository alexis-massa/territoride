//! Decay: everything a player holds loses value linearly and is gone after [`DECAY_THRESHOLD_DAYS`].

use chrono::{DateTime, Utc};

/// Captures have no value after this many days.
pub const DECAY_THRESHOLD_DAYS: i64 = 30;

/// Whole calendar days (UTC) elapsed since a capture.
pub fn elapsed_days(captured_at: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now.date_naive() - captured_at.date_naive()).num_days()
}

/// `base_value` decayed linearly to 0 at [`DECAY_THRESHOLD_DAYS`]; 0 if unowned.
pub fn current_value(
    base_value: f64,
    captured_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> f64 {
    let Some(at) = captured_at else { return 0.0 };
    let remaining = (1.0 - elapsed_days(at, now) as f64 / DECAY_THRESHOLD_DAYS as f64).max(0.0);
    base_value * remaining
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn value_decays_linearly_by_calendar_day() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 0, 5, 0).unwrap();
        let yesterday_late = Utc.with_ymd_and_hms(2026, 11, 14, 23, 59, 0).unwrap();
        assert_eq!(
            elapsed_days(yesterday_late, now),
            1,
            "whole calendar days, not 24-hour periods"
        );
        assert_eq!(current_value(30.0, Some(now), now), 30.0);
        let fifteen = now - chrono::Duration::days(15);
        assert_eq!(current_value(30.0, Some(fifteen), now), 15.0);
        assert_eq!(
            current_value(30.0, Some(now - chrono::Duration::days(45)), now),
            0.0
        );
        assert_eq!(current_value(30.0, None, now), 0.0);
    }
}
