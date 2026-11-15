//! A scripted game, step by step, against what the Python version recorded for the same game
//! (`scripts/gen_scenario_fixture.py`): imports (a late-synced older ride, odd activities, a repeated import),
//! a release, POIs added later, expiry. After each step the cells, POIs, visits and the leaderboard at six
//! different dates must be identical. Needs the local Postgres (`just test`).

mod common;

use std::collections::HashMap;
use std::f64::consts::PI;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use territoride::capture::{recapture_all, release_activity, release_expired};
use territoride::import::{ImportCounts, StravaActivity, import_activities};
use territoride::scoring::leaderboard;

const FIXTURE: &str = include_str!("fixtures/scenario.json");

fn iso(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn line(lat: f64, lng: f64, bearing: f64, km: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let d = km * 1000.0 * i as f64 / (n - 1) as f64;
            (
                lat + d * bearing.cos() / 111_000.0,
                lng + d * bearing.sin() / (111_000.0 * lat.to_radians().cos()),
            )
        })
        .collect()
}

fn ring(lat: f64, lng: f64, radius_m: f64, n: usize) -> Vec<(f64, f64)> {
    let mut pts: Vec<_> = (0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / n as f64;
            (
                lat + radius_m * a.sin() / 111_000.0,
                lng + radius_m * a.cos() / (111_000.0 * lat.to_radians().cos()),
            )
        })
        .collect();
    pts.push(pts[0]);
    pts
}

/// Google polyline encoding, the inverse of `polyline::decode`.
fn encode(points: &[(f64, f64)]) -> String {
    let (mut out, mut prev) = (String::new(), (0i64, 0i64));
    for (lat, lng) in points {
        let cur = (
            (lat * 1e5).round_ties_even() as i64,
            (lng * 1e5).round_ties_even() as i64,
        );
        for d in [cur.0 - prev.0, cur.1 - prev.1] {
            let mut v = if d < 0 { !(d << 1) } else { d << 1 };
            while v >= 0x20 {
                out.push((((0x20 | (v & 0x1f)) + 63) as u8) as char);
                v >>= 5;
            }
            out.push(((v + 63) as u8) as char);
        }
        prev = cur;
    }
    out
}

/// A Strava activity summary being put together.
struct Act(Value);

impl Act {
    fn track(mut self, points: &[(f64, f64)]) -> Self {
        self.0["map"] = json!({"summary_polyline": encode(points)});
        self
    }

    fn start(mut self, (lat, lng): (f64, f64)) -> Self {
        self.0["start_latlng"] = json!([lat, lng]);
        self
    }

    fn done(self) -> StravaActivity {
        serde_json::from_value(self.0).unwrap()
    }
}

struct Scenario {
    now: DateTime<Utc>,
    pool: PgPool,
    users: HashMap<&'static str, i64>,
}

impl Scenario {
    fn raw(&self, id: i64, name: &str, sport: &str, days_ago: i64, distance: f64) -> Act {
        Act(
            json!({"id": id, "name": name, "sport_type": sport, "distance": distance,
            "start_date": iso(self.now - Duration::days(days_ago))}),
        )
    }

    async fn import(&self, user: &str, rows: Vec<Act>) -> ImportCounts {
        let rows: Vec<StravaActivity> = rows.into_iter().map(Act::done).collect();
        import_activities(&self.pool, self.users[user], &rows)
            .await
            .unwrap()
    }

    async fn state(&self) -> Value {
        let q = |sql: &'static str| async move {
            sqlx::query_as::<
                _,
                (
                    String,
                    Option<String>,
                    Option<String>,
                    Option<DateTime<Utc>>,
                ),
            >(sql)
            .fetch_all(&self.pool)
            .await
            .unwrap()
        };
        let mut cells: Vec<Value> = q("SELECT c.cell_id, u.username, a.name, c.captured_at FROM game_territorycell c
                JOIN accounts_user u ON u.id = c.owner_id LEFT JOIN game_activity a ON a.id = c.captured_by_id")
            .await
            .into_iter()
            .map(|(c, u, a, t)| json!([c, u, a, t.map(iso)]))
            .collect();
        let mut pois: Vec<Value> = q("SELECT p.name, u.username, a.name, p.claimed_at FROM game_poi p
                LEFT JOIN accounts_user u ON u.id = p.owner_id LEFT JOIN game_activity a ON a.id = p.claimed_by_id")
            .await
            .into_iter()
            .map(|(n, u, a, t)| json!([n, u, a, t.map(iso)]))
            .collect();
        let mut visits: Vec<Value> = sqlx::query_as::<_, (String, String, Option<String>, DateTime<Utc>)>(
            "SELECT u.username, v.cell_id, a.name, v.visited_at FROM game_cellvisit v
             JOIN accounts_user u ON u.id = v.user_id LEFT JOIN game_activity a ON a.id = v.activity_id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|(u, c, a, t)| json!([u, c, a, iso(t)]))
        .collect();
        let mut activities: Vec<String> = sqlx::query_scalar("SELECT name FROM game_activity")
            .fetch_all(&self.pool)
            .await
            .unwrap();
        for list in [&mut cells, &mut pois, &mut visits] {
            list.sort_by_key(Value::to_string);
        }
        activities.sort();
        json!({"cells": cells, "pois": pois, "visits": visits, "activities": activities})
    }

    async fn boards(&self) -> Value {
        let mut out = Vec::new();
        for days in [0, 10, 25, 29, 31, 45] {
            let rows = leaderboard(&self.pool, self.now + Duration::days(days), true)
                .await
                .unwrap();
            out.push(json!({"days": days as f64, "rows": normalised(serde_json::to_value(rows).unwrap())}));
        }
        Value::Array(out)
    }
}

/// Timestamps and whole-number floats are written differently by the two serialisers; compare their values.
fn normalised(v: Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.into_iter().map(normalised).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalised(v))).collect()),
        Value::String(s) => match DateTime::parse_from_rfc3339(&s) {
            Ok(d) => Value::String(iso(d.with_timezone(&Utc))),
            Err(_) => Value::String(s),
        },
        Value::Number(n) => n.as_f64().map_or(Value::Number(n.clone()), |f| json!(f)),
        other => other,
    }
}

#[tokio::test]
async fn the_scripted_game_matches_python() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let now: DateTime<Utc> = fixture["now"].as_str().unwrap().parse().unwrap();
    let pool = common::fresh_db().await;
    let mut users = HashMap::new();
    for name in ["ann", "bob", "cy"] {
        users.insert(name, common::add_user(&pool, name).await);
    }
    let s = Scenario { now, pool, users };

    let ride_a = line(45.90, 6.10, 0.3, 14.0, 40);
    let ride_old = line(45.88, 6.14, 2.2, 12.0, 40);
    let loop_b = ring(45.93, 6.15, 2500.0, 36);
    let hike_c = line(45.91, 6.12, 5.0, 5.0, 25);
    let swim_lake = ring(45.85, 6.20, 300.0, 16);
    for (name, kind, alt, (lat, lng)) in [
        ("Col A", "mountain_pass", 1500, ride_a[20]),
        (
            "Col near",
            "mountain_pass",
            1200,
            (ride_a[20].0 + 0.0006, ride_a[20].1),
        ),
        (
            "Col far",
            "mountain_pass",
            900,
            (ride_a[20].0 + 0.004, ride_a[20].1),
        ),
        ("Cime B", "summit", 2200, loop_b[9]),
        ("Cime nowhere", "summit", 3100, (46.5, 7.5)),
    ] {
        add_poi(&s.pool, name, kind, alt, lat, lng).await;
    }

    let mut results: Vec<Option<ImportCounts>> = Vec::new();
    let mut run = 0usize;
    let steps = fixture["steps"].as_array().unwrap();
    macro_rules! check {
        ($result:expr) => {{
            let step = &steps[run];
            run += 1;
            let label = step["label"].as_str().unwrap();
            if let Some(r) = step.get("result") {
                let got = $result.unwrap_or_default();
                assert_eq!(
                    json!([got.activities, got.cells, got.pois]),
                    *r,
                    "import result, {label}"
                );
            }
            assert_eq!(
                s.state().await,
                normalised(step["state"].clone()),
                "state after: {label}"
            );
            assert_eq!(
                s.boards().await,
                normalised(step["boards"].clone()),
                "leaderboards after: {label}"
            );
            results.push($result);
        }};
    }

    check!(None::<ImportCounts>);
    let r = s
        .import(
            "ann",
            vec![s.raw(1001, "A ride", "Ride", 6, 20_000.0).track(&ride_a)],
        )
        .await;
    check!(Some(r));
    let r = s
        .import(
            "bob",
            vec![s.raw(1002, "B loop", "Run", 3, 4_000.0).track(&loop_b)],
        )
        .await;
    check!(Some(r));
    let r = s
        .import(
            "cy",
            vec![s.raw(1003, "C hike", "Hike", 3, 5_000.0).track(&hike_c)],
        )
        .await;
    check!(Some(r));
    let r = s
        .import(
            "ann",
            vec![
                s.raw(1004, "A old ride", "Ride", 12, 12_000.0)
                    .track(&ride_old),
            ],
        )
        .await;
    check!(Some(r));
    let r = s
        .import(
            "cy",
            vec![
                s.raw(1005, "Lake swim", "Swim", 2, 1_500.0)
                    .track(&swim_lake),
                s.raw(1006, "Pool swim", "Swim", 2, 1_000.0)
                    .start((45.80, 6.30)),
                s.raw(1007, "Open water, no gps", "OpenWaterSwim", 1, 2_000.0),
                s.raw(1008, "Ride, no gps", "Ride", 1, 9_000.0),
                s.raw(1009, "Ride, one point", "Ride", 1, 9_000.0)
                    .track(&[(45.7, 6.4)]),
                s.raw(1010, "Run on a treadmill start", "Run", 1, 5_000.0)
                    .start((45.71, 6.41)),
            ],
        )
        .await;
    check!(Some(r));
    let r = s
        .import(
            "ann",
            vec![s.raw(1001, "A ride", "Ride", 6, 20_000.0).track(&ride_a)],
        )
        .await;
    check!(Some(r));

    let id: i64 = sqlx::query_scalar("SELECT id FROM game_activity WHERE name = 'B loop'")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    release_activity(&s.pool, id).await.unwrap();
    check!(None::<ImportCounts>);

    add_poi(
        &s.pool,
        "Col late",
        "mountain_pass",
        1800,
        hike_c[10].0,
        hike_c[10].1,
    )
    .await;
    recapture_all(&s.pool).await.unwrap();
    check!(None::<ImportCounts>);

    for days in [20, 28, 40] {
        let released = release_expired(&s.pool, now + Duration::days(days))
            .await
            .unwrap();
        assert_eq!(
            json!(released),
            steps[run]["released"],
            "released at +{days} days"
        );
        check!(None::<ImportCounts>);
    }
    assert_eq!(run, steps.len());
}

async fn add_poi(pool: &PgPool, name: &str, kind: &str, altitude: i32, lat: f64, lng: f64) {
    sqlx::query("INSERT INTO game_poi (name, poi_type, altitude_m, location) VALUES ($1, $2, $3, ST_SetSRID(ST_MakePoint($4, $5), 4326))")
        .bind(name)
        .bind(kind)
        .bind(altitude)
        .bind(lng)
        .bind(lat)
        .execute(pool)
        .await
        .unwrap();
}
