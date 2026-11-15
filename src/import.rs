//! Turning Strava activity summaries into stored activities and captured territory.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use pi_common::{ApiResult, AppError};
use serde::Deserialize;
use sqlx::PgPool;

use crate::capture::{self, ActivityRef, NewActivity};
use crate::grid::LatLon;
use crate::polyline;
use crate::scoring::SWIM_SPORT_TYPES;

/// The fields of Strava's activity summary the game uses.
#[derive(Debug, Clone, Deserialize)]
pub struct StravaActivity {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub sport_type: String,
    pub start_date: String,
    pub distance: Option<f64>,
    pub map: Option<StravaMap>,
    pub start_latlng: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StravaMap {
    pub polyline: Option<String>,
    pub summary_polyline: Option<String>,
}

/// What an import stored.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImportCounts {
    pub activities: u64,
    pub cells: u64,
    pub pois: u64,
}

impl std::ops::AddAssign for ImportCounts {
    fn add_assign(&mut self, o: Self) {
        self.activities += o.activities;
        self.cells += o.cells;
        self.pois += o.pois;
    }
}

/// The track an activity gives. `None` outside the swims means the activity is skipped; a swim without any
/// location is still kept (scored by distance) but holds no tile.
enum Track {
    Skip,
    NoLocation,
    Points(Vec<LatLon>),
}

fn track_of(a: &StravaActivity) -> Track {
    let encoded = a.map.as_ref().and_then(|m| {
        m.polyline
            .as_deref()
            .filter(|p| !p.is_empty())
            .or(m.summary_polyline.as_deref().filter(|p| !p.is_empty()))
    });
    if let Some(encoded) = encoded {
        let points = polyline::decode(encoded);
        return if points.len() < 2 {
            Track::Skip
        } else {
            Track::Points(points)
        };
    }
    // A start point without a line (a pool swim: signal before and after, none underwater) holds its one tile.
    if let Some([lat, lng]) = a.start_latlng.as_deref() {
        return Track::Points(vec![(*lat, *lng); 2]);
    }
    if SWIM_SPORT_TYPES.contains(&a.sport_type.as_str()) {
        Track::NoLocation
    } else {
        Track::Skip
    }
}

/// Store the activities Strava reports that are not stored yet and capture what their tracks hold.
/// Activities with no location (other than swims) are skipped. Everything is stored or nothing is.
pub async fn import_activities(
    pool: &PgPool,
    user_id: i64,
    activities: &[StravaActivity],
) -> ApiResult<ImportCounts> {
    let mut tx = pool.begin().await?;
    let mut known: HashSet<i64> = sqlx::query_scalar!(
        "SELECT strava_activity_id FROM game_activity WHERE strava_activity_id IS NOT NULL"
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .flatten()
    .collect();
    let mut counts = ImportCounts::default();
    for a in activities {
        if known.contains(&a.id) {
            continue;
        }
        let track = track_of(a);
        if matches!(track, Track::Skip) {
            continue;
        }
        let recorded_at = DateTime::parse_from_rfc3339(&a.start_date)
            .map_err(|e| AppError::BadRequest(format!("activity {}: bad start_date: {e}", a.id)))?
            .with_timezone(&Utc);
        let points = match &track {
            Track::Points(p) => Some(p.as_slice()),
            _ => None,
        };
        let id = capture::create_activity(
            &mut tx,
            &NewActivity {
                user_id,
                name: &a.name,
                track: points,
                recorded_at,
                strava_activity_id: Some(a.id),
                sport_type: &a.sport_type,
                distance_m: a.distance.map(|d| d.round_ties_even() as i32),
            },
        )
        .await?;
        known.insert(a.id);
        counts.activities += 1;
        if let Some(points) = points {
            let (cells, pois) = capture::capture_activity(
                &mut tx,
                &ActivityRef {
                    id,
                    user_id,
                    recorded_at,
                },
                points,
            )
            .await?;
            counts.cells += cells;
            counts.pois += pois;
        }
    }
    tx.commit().await?;
    Ok(counts)
}
