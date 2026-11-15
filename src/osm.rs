//! Mountain passes and summits from OpenStreetMap: loaded from a file extracted with
//! `scripts/extract_osm_pois.py`, or fetched from the Overpass API (worldwide, in regions that are split in
//! four when Overpass times out; what is left is saved so a later run resumes).

use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_common::{ApiResult, AppError};
use rand::seq::SliceRandom;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

const OVERPASS_URL: &str = "https://overpass-api.de/api/interpreter";
const MAX_ATTEMPTS: u32 = 3;
const BASE_PACE: f64 = 2.0;
const MAX_PACE: f64 = 60.0;
const QUERY_TIMEOUT_S: u32 = 25;
const HTTP_TIMEOUT: Duration = Duration::from_secs(40);
const MAX_SPLIT_DEPTH: u32 = 20;
const MAX_TOTAL_REQUESTS: u32 = 100_000;
const SUMMIT_MIN_ELEVATION_M: u32 = 1000;
const BATCH: usize = 1000;
/// Overpass rejects requests that do not say who they are.
const USER_AGENT: &str = "TerritoRide/0.1 (hobby project; not for production use)";

/// `(west, south, east, north, depth)`; the pending file stores a list of these as JSON arrays.
type Region = (f64, f64, f64, f64, u32);
const WORLD: Region = (-180.0, -90.0, 180.0, 90.0, 0);

/// The two kinds of point of interest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PoiType {
    MountainPass,
    Summit,
}

impl PoiType {
    pub fn as_db(self) -> &'static str {
        match self {
            Self::MountainPass => "mountain_pass",
            Self::Summit => "summit",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MountainPass => "Mountain pass",
            Self::Summit => "Summit",
        }
    }

    /// The node filter of the Overpass query.
    fn statement(self) -> String {
        match self {
            Self::MountainPass => r#"node["mountain_pass"="yes"]["name"];"#.into(),
            Self::Summit => format!(
                r#"node["natural"="peak"]["name"](if: t["ele"] != "" && (number(t["ele"]) > {SUMMIT_MIN_ELEVATION_M} || t["wikipedia"] != "" || t["prominence"] != ""));"#
            ),
        }
    }
}

/// One POI to store.
#[derive(Debug, Clone, Deserialize)]
pub struct PoiRecord {
    pub name: String,
    pub poi_type: String,
    pub lat: f64,
    pub lon: f64,
    pub altitude_m: Option<i32>,
}

/// OSM's `ele` tag in whole metres, `None` if missing or unreadable.
pub fn parse_altitude(ele: Option<&str>) -> Option<i32> {
    ele?.trim()
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .map(|f| f.round_ties_even() as i32)
}

/// Store the POIs whose (name, type) is not stored yet; returns how many were new.
pub async fn insert_new(pool: &PgPool, records: &[PoiRecord]) -> ApiResult<u64> {
    let names: Vec<String> = records.iter().map(|r| r.name.clone()).collect();
    let types: Vec<String> = records.iter().map(|r| r.poi_type.clone()).collect();
    let lons: Vec<f64> = records.iter().map(|r| r.lon).collect();
    let lats: Vec<f64> = records.iter().map(|r| r.lat).collect();
    let alts: Vec<Option<i32>> = records.iter().map(|r| r.altitude_m).collect();
    Ok(sqlx::query!(
        "INSERT INTO game_poi (name, poi_type, location, altitude_m)
         SELECT DISTINCT ON (r.name, r.poi_type) r.name, r.poi_type, ST_SetSRID(ST_MakePoint(r.lon, r.lat), 4326), r.alt
         FROM unnest($1::text[], $2::text[], $3::float8[], $4::float8[], $5::int4[]) AS r(name, poi_type, lon, lat, alt)
         WHERE NOT EXISTS (SELECT 1 FROM game_poi p WHERE p.name = r.name AND p.poi_type = r.poi_type)",
        &names,
        &types,
        &lons,
        &lats,
        &alts as &[Option<i32>]
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// Load an NDJSON file (one record per line); returns `(new, total)`.
pub async fn import_file(pool: &PgPool, path: &Path) -> ApiResult<(u64, u64)> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}: {e}", path.display())))?;
    let (mut created, mut total) = (0, 0);
    let mut batch = Vec::with_capacity(BATCH);
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        batch.push(
            serde_json::from_str::<PoiRecord>(line)
                .map_err(|e| AppError::BadRequest(format!("bad record {line:?}: {e}")))?,
        );
        if batch.len() == BATCH {
            created += insert_new(pool, &batch).await?;
            total += batch.len() as u64;
            batch.clear();
            println!("  {total} processed ({created} new so far)");
        }
    }
    created += insert_new(pool, &batch).await?;
    total += batch.len() as u64;
    Ok((created, total))
}

enum Failure {
    /// Overpass (or the network) gave up on this region: split it.
    TimedOut(String),
    /// Anything else: stop.
    Fatal(String),
}

/// The Overpass crawl.
pub struct Crawl<'a> {
    pool: &'a PgPool,
    poi_type: PoiType,
    http: reqwest::Client,
    url: String,
    pace: f64,
    requests: u32,
    fetched: u64,
    created: u64,
    stuck: Vec<Region>,
}

impl<'a> Crawl<'a> {
    pub fn new(pool: &'a PgPool, poi_type: PoiType) -> Self {
        Self::with_url(pool, poi_type, OVERPASS_URL)
    }

    pub fn with_url(pool: &'a PgPool, poi_type: PoiType, url: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("the HTTP client builds");
        Self {
            pool,
            poi_type,
            http,
            url: url.into(),
            pace: BASE_PACE,
            requests: 0,
            fetched: 0,
            created: 0,
            stuck: Vec::new(),
        }
    }

    fn pending_path(&self) -> PathBuf {
        PathBuf::from(format!(
            ".osm_import_pending_{}.json",
            self.poi_type.as_db()
        ))
    }

    /// Fetch everything, resuming from the pending file when there is one; `Ctrl-C` stops cleanly. Returns
    /// `(new, fetched)`.
    pub async fn run(mut self) -> ApiResult<(u64, u64)> {
        let path = self.pending_path();
        let mut queue: std::collections::VecDeque<Region> =
            match tokio::fs::read_to_string(&path).await {
                Ok(text) => {
                    let q: Vec<Region> = serde_json::from_str(&text).map_err(AppError::internal)?;
                    println!(
                        "Resuming {} region(s) left over from a previous run",
                        q.len()
                    );
                    q.into()
                }
                Err(_) => [WORLD].into(),
            };
        println!("Importing {} POIs worldwide", self.poi_type.label());
        let crawl = self.crawl(&mut queue, &path);
        tokio::select! {
            outcome = crawl => outcome?,
            _ = tokio::signal::ctrl_c() => println!("\nStopped - everything fetched so far is saved."),
        }
        let remaining: Vec<Region> = queue
            .into_iter()
            .chain(self.stuck.iter().copied())
            .collect();
        if remaining.is_empty() {
            let _ = tokio::fs::remove_file(&path).await;
        } else {
            self.save(&path, &remaining).await?;
            println!(
                "{} region(s) still pending in {} - run this command again to pick up where it left off.",
                remaining.len(),
                path.display()
            );
        }
        Ok((self.created, self.fetched))
    }

    async fn save(&self, path: &Path, regions: &[Region]) -> ApiResult<()> {
        tokio::fs::write(
            path,
            serde_json::to_vec(regions).map_err(AppError::internal)?,
        )
        .await
        .map_err(AppError::internal)
    }

    async fn crawl(
        &mut self,
        queue: &mut std::collections::VecDeque<Region>,
        path: &Path,
    ) -> ApiResult<()> {
        while let Some(&(west, south, east, north, depth)) = queue.front() {
            if self.requests >= MAX_TOTAL_REQUESTS {
                return Ok(());
            }
            let query = format!(
                "[out:json][timeout:{QUERY_TIMEOUT_S}][bbox:{south},{west},{north},{east}];{}out body;",
                self.poi_type.statement()
            );
            let indent = "  ".repeat(depth as usize);
            match self.fetch(&query, &indent).await {
                Ok(elements) => {
                    self.store(&elements, &indent).await?;
                    queue.pop_front();
                }
                Err(Failure::TimedOut(reason)) if depth >= MAX_SPLIT_DEPTH => {
                    println!(
                        "{indent}x ({west:.1},{south:.1})-({east:.1},{north:.1}) still failing ({reason}), deferring"
                    );
                    self.stuck.extend(queue.pop_front());
                }
                Err(Failure::TimedOut(_)) => {
                    println!(
                        "{indent}~ ({west:.1},{south:.1})-({east:.1},{north:.1}) splitting..."
                    );
                    queue.pop_front();
                    let (mid_lon, mid_lat) = ((west + east) / 2.0, (south + north) / 2.0);
                    let mut quadrants = [
                        (west, south, mid_lon, mid_lat, depth + 1),
                        (mid_lon, south, east, mid_lat, depth + 1),
                        (west, mid_lat, mid_lon, north, depth + 1),
                        (mid_lon, mid_lat, east, north, depth + 1),
                    ];
                    quadrants.shuffle(&mut rand::rng()); // don't always starve the same quadrant on cutoff
                    queue.extend(quadrants);
                }
                Err(Failure::Fatal(reason)) => return Err(AppError::internal(reason)),
            }
            let pending: Vec<Region> = queue
                .iter()
                .copied()
                .chain(self.stuck.iter().copied())
                .collect();
            self.save(path, &pending).await?;
        }
        Ok(())
    }

    /// Store a fetched batch at once, so nothing is lost if the run is stopped.
    async fn store(&mut self, elements: &[Value], indent: &str) -> ApiResult<()> {
        let records: Vec<PoiRecord> = elements
            .iter()
            .filter_map(|e| {
                Some(PoiRecord {
                    name: e["tags"]["name"].as_str()?.to_owned(),
                    poi_type: self.poi_type.as_db().to_owned(),
                    lat: e["lat"].as_f64()?,
                    lon: e["lon"].as_f64()?,
                    altitude_m: parse_altitude(e["tags"]["ele"].as_str()),
                })
            })
            .collect();
        if records.is_empty() {
            return Ok(());
        }
        self.created += insert_new(self.pool, &records).await?;
        self.fetched += records.len() as u64;
        println!("{indent}+ {} ({} new so far)", records.len(), self.created);
        Ok(())
    }

    /// POST the query; only 429 is retried in place (and slows the pace), anything else that fails splits the region.
    async fn fetch(&mut self, query: &str, indent: &str) -> Result<Vec<Value>, Failure> {
        for attempt in 1..=MAX_ATTEMPTS {
            if self.requests > 0 {
                tokio::time::sleep(Duration::from_secs_f64(self.pace)).await;
            }
            self.requests += 1;
            let response = self
                .http
                .post(&self.url)
                .form(&[("data", query)])
                .send()
                .await
                .map_err(|e| Failure::TimedOut(e.to_string()))?;
            let status = response.status();
            if status.is_success() {
                let payload: Value = response
                    .json()
                    .await
                    .map_err(|e| Failure::TimedOut(e.to_string()))?;
                if let Some(remark) = payload.get("remark") {
                    return Err(Failure::TimedOut(remark.to_string()));
                }
                self.pace = (self.pace / 2.0).max(BASE_PACE);
                return Ok(payload["elements"].as_array().cloned().unwrap_or_default());
            }
            if status.as_u16() != 429 {
                return Err(if status.is_client_error() {
                    Failure::Fatal(format!("HTTP {status}"))
                } else {
                    Failure::TimedOut(format!("HTTP {status}"))
                });
            }
            let new_pace = (self.pace * 2.0).min(MAX_PACE);
            if new_pace > self.pace {
                println!("{indent}! backing off to {new_pace:.0}s between requests");
            }
            self.pace = new_pace;
            if attempt < MAX_ATTEMPTS {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<f64>().ok());
                println!("{indent}! 429, retrying ({attempt}/{MAX_ATTEMPTS})...");
                tokio::time::sleep(Duration::from_secs_f64(wait.unwrap_or(self.pace))).await;
            }
        }
        Err(Failure::TimedOut(format!(
            "HTTP 429 after {MAX_ATTEMPTS} attempts"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altitudes_round_like_python_and_ignore_junk() {
        assert_eq!(parse_altitude(Some("2754.4")), Some(2754));
        assert_eq!(parse_altitude(Some("2754.5")), Some(2754));
        assert_eq!(parse_altitude(Some("2755.5")), Some(2756));
        assert_eq!(parse_altitude(Some("1 200 m")), None);
        assert_eq!(parse_altitude(None), None);
    }
}
