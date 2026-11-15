//! Shared helpers for the integration tests: a fresh migrated database per test.
//! Run with `just test` (it starts the local Postgres and sets `TEST_DATABASE_URL`).
#![allow(dead_code)] // each test binary uses a different subset

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use http_body_util::BodyExt;
use pi_common::cli::Context;
use pi_common::config::Config;
use pi_common::server::Security;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
use territoride::crypto::TokenCipher;
use territoride::routes;
use territoride::settings::Settings;
use territoride::strava::Strava;
use tower::ServiceExt;

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

// ---- geometry for synthetic tracks (nothing here is a real player's data) ----

/// `n` points along a straight line from `(lat, lng)` heading `bearing` radians for `km`.
pub fn line(lat: f64, lng: f64, bearing: f64, km: f64, n: usize) -> Vec<(f64, f64)> {
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

/// A closed ring of `n` points around `(lat, lng)`.
pub fn ring(lat: f64, lng: f64, radius_m: f64, n: usize) -> Vec<(f64, f64)> {
    let mut pts: Vec<_> = (0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
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
pub fn encode(points: &[(f64, f64)]) -> String {
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

// ---- the app, over HTTP ----

pub const FERNET_KEY: &str = "CE7oj2eOJ-Es3tI_KD7y5xlo370XPgjPvEERoJu3mD4=";
pub const VERIFY_TOKEN: &str = "verify-me";

/// A stand-in for Strava's servers: records what it was asked and answers from `activities`.
pub struct MockStrava {
    pub url: String,
    pub log: Arc<Mutex<Vec<String>>>,
    pub activities: Arc<Mutex<Vec<Value>>>,
}

impl MockStrava {
    pub async fn start() -> Self {
        let log = Arc::new(Mutex::new(Vec::new()));
        let activities = Arc::new(Mutex::new(Vec::new()));
        let state = (log.clone(), activities.clone());
        let app = Router::new()
            .route(
                "/oauth/token",
                post(|State((log, _)): State<MockState>, Form(f): Form<HashMap<String, String>>| async move {
                    let grant = f["grant_type"].clone();
                    log.lock().unwrap().push(format!("token:{grant}"));
                    let n = if grant == "refresh_token" { 2 } else { 1 };
                    let expires_at = if f.get("code").is_some_and(|c| c == "expired") { 1 } else { 4_102_444_800i64 };
                    Json(json!({"access_token": format!("acc-{n}"), "refresh_token": format!("ref-{n}"), "expires_at": expires_at,
                        "athlete": {"id": 777, "username": "rider", "firstname": "Ri", "lastname": "Der"}}))
                }),
            )
            .route(
                "/api/v3/athlete/activities",
                get(|State((log, acts)): State<MockState>, Query(q): Query<HashMap<String, String>>| async move {
                    log.lock().unwrap().push(format!("list:page{}", q["page"]));
                    Json(if q["page"] == "1" { acts.lock().unwrap().clone() } else { vec![] })
                }),
            )
            .route(
                "/api/v3/activities/{id}",
                get(|State((log, acts)): State<MockState>, axum::extract::Path(id): axum::extract::Path<i64>| async move {
                    log.lock().unwrap().push(format!("activity:{id}"));
                    acts.lock().unwrap().iter().find(|a| a["id"] == id).cloned().map(Json).ok_or(StatusCode::NOT_FOUND)
                }),
            )
            .route(
                "/oauth/deauthorize",
                post(|State((log, _)): State<MockState>, Form(f): Form<HashMap<String, String>>| async move {
                    log.lock().unwrap().push(format!("deauthorize:{}", f["access_token"]));
                    Json(json!({}))
                }),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            log,
            activities,
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

type MockState = (Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<Value>>>);

/// A Strava activity summary with a polyline.
pub fn strava_activity(
    id: i64,
    name: &str,
    sport: &str,
    days_ago: i64,
    points: &[(f64, f64)],
) -> Value {
    json!({"id": id, "name": name, "sport_type": sport, "distance": 10_000.0,
        "start_date": (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339(),
        "map": {"summary_polyline": encode(points)}})
}

pub struct Env {
    pub pool: PgPool,
    pub app: Router,
    pub strava: MockStrava,
}

impl Env {
    pub async fn new() -> Self {
        let pool = fresh_db().await;
        let strava = MockStrava::start().await;
        let config = Config {
            database_url: String::new(),
            app_domain: "app.test".into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            cookie_secure: true,
            session_days: 30,
        };
        let settings = Settings {
            strava_client_id: "42".into(),
            strava_client_secret: "secret".into(),
            webhook_verify_token: VERIFY_TOKEN.into(),
            cipher: TokenCipher::new(FERNET_KEY).unwrap(),
            decay_hour_utc: None,
        };
        let client = Strava::with_base("42", "secret", &strava.url);
        let router = routes::build_with(
            Context {
                config,
                pool: pool.clone(),
            },
            &settings,
            client,
        )
        .await
        .unwrap();
        let app = pi_common::server::wrap(router, pool.clone(), Security::with_csp(routes::CSP));
        Self { pool, app, strava }
    }

    pub fn client(&self) -> Client {
        Client {
            app: self.app.clone(),
            cookies: Default::default(),
            csrf: None,
        }
    }

    /// A player (no Strava), logged in.
    pub async fn login(&self, name: &str) -> (Client, i64) {
        let id = add_user(&self.pool, name).await;
        let mut client = self.client();
        let (jar, session) = pi_common::auth::Sessions::new(
            self.pool.clone(),
            pi_common::auth::SessionOptions {
                table: "app_sessions",
                cookie_name: "territoride_session",
                days: 30,
                secure: true,
            },
        )
        .start(axum_extra::extract::cookie::CookieJar::new(), id)
        .await
        .unwrap();
        for c in jar.iter() {
            client
                .cookies
                .insert(c.name().to_owned(), c.value().to_owned());
        }
        client.csrf = Some(session.csrf_token);
        (client, id)
    }
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }

    pub fn location(&self) -> String {
        self.headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    }
}

/// A browser: keeps cookies and the CSRF token.
pub struct Client {
    app: Router,
    pub cookies: HashMap<String, String>,
    pub csrf: Option<String>,
}

impl Client {
    pub async fn get(&mut self, uri: &str) -> Resp {
        self.send("GET", uri, Body::empty(), None).await
    }

    pub async fn post_json(&mut self, uri: &str, body: Value) -> Resp {
        self.send(
            "POST",
            uri,
            Body::from(body.to_string()),
            Some("application/json"),
        )
        .await
    }

    pub async fn post(&mut self, uri: &str) -> Resp {
        self.send("POST", uri, Body::empty(), None).await
    }

    pub async fn post_multipart(
        &mut self,
        uri: &str,
        field: &str,
        file_name: &str,
        content: &str,
    ) -> Resp {
        let boundary = "XBOUNDARYX";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{file_name}\"\r\nContent-Type: application/gpx+xml\r\n\r\n{content}\r\n--{boundary}--\r\n"
        );
        self.send(
            "POST",
            uri,
            Body::from(body),
            Some(&format!("multipart/form-data; boundary={boundary}")),
        )
        .await
    }

    async fn send(
        &mut self,
        method: &str,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
    ) -> Resp {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "app.test");
        if !self.cookies.is_empty() {
            let cookie = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            req = req.header(header::COOKIE, cookie);
        }
        if let Some(csrf) = &self.csrf {
            req = req.header("x-csrf-token", csrf);
        }
        if let Some(ct) = content_type {
            req = req.header(header::CONTENT_TYPE, ct);
        }
        if method != "GET" {
            req = req.header(header::ORIGIN, "https://app.test");
        }
        let mut req = req.body(body).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 4000))));
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        for value in parts.headers.get_all(header::SET_COOKIE) {
            let pair = value.to_str().unwrap().split(';').next().unwrap();
            let (name, val) = pair.split_once('=').unwrap();
            if val.is_empty() {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), val.to_owned());
            }
        }
        let body = body.collect().await.unwrap().to_bytes().to_vec();
        Resp {
            status: parts.status,
            headers: parts.headers,
            body,
        }
    }
}
