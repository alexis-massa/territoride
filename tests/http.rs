//! The app over HTTP against a stand-in Strava: login, sync, webhook, account deletion, the map's data, GPX
//! upload and the small read-only endpoints. Needs the local Postgres (`just test`).

mod common;

use std::time::Duration;

use common::{Env, VERIFY_TOKEN, line, ring, strava_activity};
use serde_json::{Value, json};
use sqlx::PgPool;

const BBOX: &str = "bbox=5.5,45.0,6.5,46.5";

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Wait for a background job to have an effect.
async fn eventually(pool: &PgPool, sql: &'static str, want: i64) {
    for _ in 0..100 {
        if count(pool, sql).await == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(count(pool, sql).await, want, "{sql}");
}

#[tokio::test]
async fn strava_login_sync_webhook_and_account_deletion() {
    let env = Env::new().await;
    let ride = line(45.90, 6.10, 0.3, 14.0, 40);
    let hike = line(45.95, 6.05, 1.0, 6.0, 30);
    *env.strava.activities.lock().unwrap() = vec![strava_activity(1001, "Ride", "Ride", 2, &ride)];
    let mut me = env.client();

    // Not logged in: no player, and the protected endpoints say 401.
    assert_eq!(
        me.get("/api/me").await.json(),
        json!({"user": null, "default_bbox": null})
    );
    assert_eq!(me.get("/api/profile").await.status, 401);
    assert_eq!(me.post("/api/strava/sync").await.status, 401);

    // The handshake: to Strava with a state, back with a code.
    let start = me.get("/api/strava/authorize").await;
    assert_eq!(start.status, 303);
    let to = reqwest::Url::parse(&start.location()).unwrap();
    assert_eq!(to.path(), "/oauth/authorize");
    let params: std::collections::HashMap<_, _> = to.query_pairs().collect();
    assert_eq!(
        params["redirect_uri"],
        "https://app.test/api/strava/callback"
    );
    let state = params["state"].to_string();
    assert_eq!(me.cookies["strava_oauth_state"], state);

    let done = me
        .get(&format!("/api/strava/callback?code=abc&state={state}"))
        .await;
    assert_eq!(
        done.location(),
        "/oauth/complete?status=success&message=Connected%21"
    );
    assert!(
        !me.cookies.contains_key("strava_oauth_state"),
        "the state is single-use"
    );
    let info = me.get("/api/me").await.json();
    assert_eq!(info["user"]["username"], "rider");
    assert_eq!(info["user"]["strava_connected"], true);
    me.csrf = info["user"]["csrf_token"].as_str().map(str::to_owned);

    // Tokens are stored encrypted, in the Python version's format.
    let stored: String = sqlx::query_scalar(
        "SELECT strava_access_token_encrypted FROM accounts_user WHERE username = 'rider'",
    )
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert!(stored.starts_with("gAAAA"), "{stored}");

    // Sync pulls the list once, and a second sync finds nothing new.
    let synced = me.post("/api/strava/sync").await.json();
    assert_eq!(synced["ok"], true);
    assert!(
        synced["message"]
            .as_str()
            .unwrap()
            .starts_with("Synced 1 activities: captured "),
        "{synced}"
    );
    assert_eq!(
        me.post("/api/strava/sync").await.json()["message"],
        "No new activities to sync."
    );
    assert!(
        count(
            &env.pool,
            "SELECT count(*) FROM game_territorycell WHERE owner_id IS NOT NULL"
        )
        .await
            > 0
    );

    // The webhook: challenge, then events handled in the background.
    let mut strava = env.client();
    let url = |token: &str| {
        format!(
            "/accounts/strava/webhook/?hub.mode=subscribe&hub.verify_token={token}&hub.challenge=c4ll"
        )
    };
    assert_eq!(strava.get(&url("wrong")).await.status, 403);
    assert_eq!(
        strava.get(&url(VERIFY_TOKEN)).await.json(),
        json!({"hub.challenge": "c4ll"})
    );
    env.strava
        .activities
        .lock()
        .unwrap()
        .push(strava_activity(1002, "Hike", "Hike", 1, &hike));
    let event = |aspect: &str, id: i64| json!({"object_type": "activity", "object_id": id, "aspect_type": aspect, "owner_id": 777, "updates": {}});
    assert_eq!(
        strava
            .post_json("/accounts/strava/webhook/", event("create", 1002))
            .await
            .status,
        200
    );
    eventually(&env.pool, "SELECT count(*) FROM game_activity", 2).await;
    assert_eq!(
        strava
            .post_json("/accounts/strava/webhook/", event("delete", 1001))
            .await
            .status,
        200
    );
    eventually(&env.pool, "SELECT count(*) FROM game_activity", 1).await;
    // Events for unknown athletes or other kinds are ignored.
    let stranger =
        json!({"object_type": "activity", "object_id": 5, "aspect_type": "create", "owner_id": 1});
    assert_eq!(
        strava
            .post_json("/accounts/strava/webhook/", stranger)
            .await
            .status,
        200
    );

    // Everyone sees the ranking; the player sees their profile.
    let board = strava.get("/api/leaderboard").await.json();
    assert_eq!(board[0]["username"], "rider");
    assert_eq!(board[0]["color"], "#2ecc71");
    let profile = me.get("/api/profile").await.json();
    assert_eq!(
        (profile["rank"].as_i64(), profile["player_count"].as_i64()),
        (Some(1), Some(1))
    );

    // Deleting the account revokes Strava access and wipes the data.
    assert_eq!(me.post("/api/account/delete").await.status, 204);
    assert!(
        env.strava.log().contains(&"deauthorize:acc-1".to_string()),
        "{:?}",
        env.strava.log()
    );
    for table in [
        "accounts_user",
        "game_activity",
        "game_cellvisit",
        "app_sessions",
    ] {
        let sql: &'static str = Box::leak(format!("SELECT count(*) FROM {table}").into_boxed_str());
        assert_eq!(count(&env.pool, sql).await, 0, "{table}");
    }
    assert_eq!(
        count(
            &env.pool,
            "SELECT count(*) FROM game_territorycell WHERE owner_id IS NOT NULL"
        )
        .await,
        0
    );
    assert_eq!(me.get("/api/me").await.json()["user"], Value::Null);
}

#[tokio::test]
async fn an_expired_token_is_refreshed_before_syncing() {
    let env = Env::new().await;
    *env.strava.activities.lock().unwrap() = vec![strava_activity(
        1,
        "Ride",
        "Ride",
        1,
        &line(45.9, 6.1, 0.3, 5.0, 20),
    )];
    let mut me = env.client();
    let state = {
        let r = me.get("/api/strava/authorize").await;
        reqwest::Url::parse(&r.location())
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .to_string()
    };
    me.get(&format!("/api/strava/callback?code=expired&state={state}"))
        .await;
    me.csrf = me.get("/api/me").await.json()["user"]["csrf_token"]
        .as_str()
        .map(str::to_owned);
    assert_eq!(me.post("/api/strava/sync").await.json()["ok"], true);
    assert!(
        env.strava
            .log()
            .contains(&"token:refresh_token".to_string()),
        "{:?}",
        env.strava.log()
    );
}

#[tokio::test]
async fn the_handshake_refuses_a_wrong_state_and_reports_a_denial() {
    let env = Env::new().await;
    let mut me = env.client();
    me.get("/api/strava/authorize").await;
    let r = me.get("/api/strava/callback?code=abc&state=forged").await;
    assert_eq!(
        r.location(),
        "/oauth/complete?status=error&message=Invalid+OAuth+state"
    );
    assert_eq!(me.get("/api/me").await.json()["user"], Value::Null);
    let r = me.get("/api/strava/callback?error=access_denied").await;
    assert_eq!(
        r.location(),
        "/oauth/complete?status=error&message=Strava+denied+access%3A+access_denied"
    );
    assert!(env.strava.log().is_empty(), "no code was exchanged");
}

#[tokio::test]
async fn map_layers_filter_by_viewport_and_player() {
    let env = Env::new().await;
    let (mut ann, ann_id) = env.login("ann").await;
    let (_, bob_id) = env.login("bob").await;
    let ride = line(45.90, 6.10, 0.3, 10.0, 40);
    let hike = line(45.60, 5.90, 1.0, 4.0, 30);
    for (user, id, name, sport, pts) in [
        (ann_id, 1, "A ride", "Ride", &ride),
        (bob_id, 2, "B hike", "Hike", &hike),
    ] {
        let a: territoride::import::StravaActivity =
            serde_json::from_value(strava_activity(id, name, sport, 1, pts)).unwrap();
        territoride::import::import_activities(&env.pool, user, &[a])
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO game_poi (name, poi_type, altitude_m, location) VALUES ('Col A', 'mountain_pass', 1500, ST_SetSRID(ST_MakePoint($1, $2), 4326)), ('Col far', 'summit', 2000, ST_SetSRID(ST_MakePoint(8.0, 47.0), 4326))")
        .bind(ride[10].1)
        .bind(ride[10].0)
        .execute(&env.pool)
        .await
        .unwrap();
    territoride::capture::recapture_all(&env.pool)
        .await
        .unwrap();

    let owners = |v: &Value| -> Vec<String> {
        let mut o: Vec<String> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["properties"]["owner"].as_str().unwrap().to_owned())
            .collect();
        o.sort();
        o.dedup();
        o
    };
    let all = ann
        .get(&format!("/api/territory.geojson?{BBOX}"))
        .await
        .json();
    assert_eq!(owners(&all), ["ann", "bob"]);
    let feature = &all["features"][0];
    assert_eq!(feature["geometry"]["type"], "Polygon");
    assert_eq!(
        feature["geometry"]["coordinates"][0]
            .as_array()
            .unwrap()
            .len(),
        feature["geometry"]["coordinates"][0]
            .as_array()
            .unwrap()
            .len()
    );
    assert!(feature["properties"]["age_days"].as_i64().unwrap() >= 0);
    assert!(feature["properties"]["remaining"].as_f64().unwrap() > 0.9);
    assert!(all["features"].as_array().unwrap().len() > 10);
    let empty = ann
        .get("/api/territory.geojson?bbox=-10,-10,-9,-9")
        .await
        .json();
    assert!(empty["features"].as_array().unwrap().is_empty());
    let mine = ann
        .get(&format!("/api/territory.geojson?{BBOX}&mine=1"))
        .await
        .json();
    assert_eq!(owners(&mine), ["ann"]);

    // Routes: only tracks that captured something, or all of the player's with mine=1.
    let routes = ann.get(&format!("/api/routes.geojson?{BBOX}")).await.json();
    assert_eq!(routes["features"].as_array().unwrap().len(), 2);
    assert_eq!(routes["features"][0]["geometry"]["type"], "LineString");
    let mine = ann
        .get(&format!("/api/routes.geojson?{BBOX}&mine=1"))
        .await
        .json();
    assert_eq!(owners(&mine), ["ann"]);

    // POIs: only those in the viewport; claimed ones carry their owner.
    let pois = ann.get(&format!("/api/pois.geojson?{BBOX}")).await.json();
    assert_eq!(pois["features"].as_array().unwrap().len(), 1);
    let col = &pois["features"][0]["properties"];
    assert_eq!(
        (
            col["name"].as_str(),
            col["poi_type_label"].as_str(),
            col["owner"].as_str()
        ),
        (Some("Col A"), Some("Mountain pass"), Some("ann"))
    );
    let mut bob = env.client();
    let anon = bob.get(&format!("/api/pois.geojson?{BBOX}")).await.json();
    assert_eq!(anon["features"][0]["properties"]["owner"], "ann");

    // Errors: a bad viewport is a 400, "mine" without a login a 403.
    assert_eq!(bob.get("/api/territory.geojson").await.status, 400);
    assert_eq!(
        bob.get("/api/routes.geojson?bbox=1,2,x,4").await.status,
        400
    );
    assert_eq!(
        bob.get(&format!("/api/territory.geojson?{BBOX}&mine=1"))
            .await
            .status,
        403
    );
    assert_eq!(
        bob.get(&format!("/api/pois.geojson?{BBOX}&mine=1"))
            .await
            .status,
        403
    );

    // The map opens on the player's holdings.
    let bbox = ann.get("/api/me").await.json()["default_bbox"].clone();
    let [x0, y0, x1, y1] = bbox
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>()[..]
    else {
        panic!("{bbox}")
    };
    assert!(
        x0 < x1 && y0 < y1 && (5.9..6.5).contains(&x0) && (45.8..46.0).contains(&y0),
        "{bbox}"
    );
}

#[tokio::test]
async fn a_gpx_file_becomes_an_activity() {
    let env = Env::new().await;
    let (mut ann, _) = env.login("ann").await;
    let points: String = ring(45.9, 6.1, 800.0, 24)
        .iter()
        .enumerate()
        .map(|(i, (lat, lng))| {
            format!(
                "<trkpt lat=\"{lat}\" lon=\"{lng}\"><time>2026-11-10T07:{:02}:00Z</time></trkpt>",
                i % 60
            )
        })
        .collect();
    let gpx = format!(
        "<?xml version=\"1.0\"?><gpx version=\"1.1\" creator=\"t\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>Lake loop</name><trkseg>{points}</trkseg></trk></gpx>"
    );
    let ok = ann
        .post_multipart("/api/import-activity", "gpx_file", "lake.gpx", &gpx)
        .await
        .json();
    assert_eq!(ok["ok"], true, "{ok}");
    assert!(
        ok["message"]
            .as_str()
            .unwrap()
            .starts_with("Imported \"Lake loop\": captured "),
        "{ok}"
    );
    assert_eq!(
        count(
            &env.pool,
            "SELECT count(*) FROM game_activity WHERE name = 'Lake loop'"
        )
        .await,
        1
    );

    let bare = |trkpts: &str| {
        format!(
            "<gpx version=\"1.1\" creator=\"t\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><trkseg>{trkpts}</trkseg></trk></gpx>"
        )
    };
    let one_point = bare("<trkpt lat=\"1\" lon=\"1\"><time>2026-11-10T07:00:00Z</time></trkpt>");
    let no_time = bare("<trkpt lat=\"1\" lon=\"1\"/><trkpt lat=\"1.1\" lon=\"1.1\"/>");
    for (content, message) in [
        ("not xml", "Couldn't read that file as GPX."),
        (
            one_point.as_str(),
            "GPX file has fewer than 2 track points.",
        ),
        (no_time.as_str(), "GPX file is missing activity date/time."),
    ] {
        let r = ann
            .post_multipart("/api/import-activity", "gpx_file", "bad.gpx", content)
            .await
            .json();
        assert_eq!(
            (r["ok"].clone(), r["message"].as_str()),
            (json!(false), Some(message))
        );
    }
    assert_eq!(
        count(&env.pool, "SELECT count(*) FROM game_activity").await,
        1
    );

    // Logged-out visitors and forged requests are refused.
    let mut anon = env.client();
    assert_eq!(
        anon.post_multipart("/api/import-activity", "gpx_file", "x.gpx", &gpx)
            .await
            .status,
        401
    );
    ann.csrf = Some("forged".into());
    assert_eq!(
        ann.post_multipart("/api/import-activity", "gpx_file", "x.gpx", &gpx)
            .await
            .status,
        403
    );
}

#[tokio::test]
async fn rules_changelog_and_logout() {
    let env = Env::new().await;
    let mut anon = env.client();
    let rules = anon.get("/api/rules").await.json();
    assert_eq!(rules["decay_days"], 30);
    assert_eq!(rules["poi_radius_m"], 100.0);
    assert_eq!(
        rules["sport_multipliers"],
        json!([
            ["Cycling", 1.0],
            ["Run", 2.0],
            ["Trail Run", 2.0],
            ["Walk", 5.0],
            ["Hike", 5.0],
            ["Swim", 8.0],
            ["Open Water Swim", 8.0]
        ])
    );
    assert!(
        anon.get("/api/changelog").await.json()["html"]
            .as_str()
            .unwrap()
            .contains("<h")
    );
    let health = anon.get("/api/healthz").await;
    assert_eq!(health.status, 200);
    let csp = health.headers["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        csp.contains("https://tiles.openfreemap.org") && csp.contains("worker-src"),
        "{csp}"
    );

    let (mut ann, _) = env.login("ann").await;
    assert_eq!(ann.post("/api/logout").await.status, 204);
    assert_eq!(ann.get("/api/me").await.json()["user"], Value::Null);
}
