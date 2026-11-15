//! Loading a GPX file as an activity (the `load-gpx` command).

use std::io::BufReader;
use std::path::Path;

use chrono::{DateTime, Utc};
use pi_common::{ApiResult, AppError};
use sqlx::PgPool;

use crate::capture::{self, ActivityRef, NewActivity};
use crate::grid::LatLon;

/// What a GPX file holds that the game uses.
#[derive(Debug, PartialEq)]
pub struct GpxTrack {
    /// The first track's name, if it has one.
    pub name: Option<String>,
    /// Every point of every segment of every track, as `(lat, lng)`.
    pub points: Vec<LatLon>,
    /// The first point's time, if it has one.
    pub started_at: Option<DateTime<Utc>>,
}

pub fn parse(reader: impl std::io::Read) -> ApiResult<GpxTrack> {
    let gpx = gpx::read(BufReader::new(reader))
        .map_err(|e| AppError::BadRequest(format!("not a GPX file: {e}")))?;
    let waypoints: Vec<_> = gpx
        .tracks
        .iter()
        .flat_map(|t| &t.segments)
        .flat_map(|s| &s.points)
        .collect();
    let started_at = waypoints
        .first()
        .and_then(|w| w.time)
        .and_then(|t| t.format().ok())
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc));
    Ok(GpxTrack {
        name: gpx
            .tracks
            .first()
            .and_then(|t| t.name.clone())
            .filter(|n| !n.is_empty()),
        points: waypoints
            .iter()
            .map(|w| (w.point().y(), w.point().x()))
            .collect(),
        started_at,
    })
}

/// Store a track as an activity of a player and capture its territory and POIs; returns `(cells, pois)`.
pub async fn store(
    pool: &PgPool,
    user_id: i64,
    name: &str,
    recorded_at: DateTime<Utc>,
    points: &[LatLon],
) -> ApiResult<(u64, u64)> {
    let mut tx = pool.begin().await?;
    let id = capture::create_activity(
        &mut tx,
        &NewActivity {
            user_id,
            name,
            track: Some(points),
            recorded_at,
            strava_activity_id: None,
            sport_type: "",
            distance_m: None,
        },
    )
    .await?;
    let activity = ActivityRef {
        id,
        user_id,
        recorded_at,
    };
    let cells = capture::capture_territory(&mut tx, &activity, points).await?;
    let pois = capture::capture_pois(&mut tx, &activity).await?;
    tx.commit().await?;
    Ok((cells, pois))
}

/// Load a GPX file from disk as an activity (the start time defaults to `now`); returns `(name, cells, pois)`.
pub async fn load(
    pool: &PgPool,
    user_id: i64,
    path: &Path,
    now: DateTime<Utc>,
) -> ApiResult<(String, u64, u64)> {
    let file = std::fs::File::open(path)
        .map_err(|e| AppError::BadRequest(format!("{}: {e}", path.display())))?;
    let track = parse(file)?;
    if track.points.len() < 2 {
        return Err(AppError::BadRequest(
            "GPX file has fewer than 2 track points".into(),
        ));
    }
    let name = track.name.unwrap_or_else(|| path.display().to_string());
    let (cells, pois) = store(
        pool,
        user_id,
        &name,
        track.started_at.unwrap_or(now),
        &track.points,
    )
    .await?;
    Ok((name, cells, pois))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPX: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" creator="t" xmlns="http://www.topografix.com/GPX/1/1">
 <trk><name>Morning ride</name><trkseg>
  <trkpt lat="45.1" lon="6.1"><time>2026-11-01T07:00:00Z</time></trkpt>
  <trkpt lat="45.2" lon="6.2"></trkpt>
 </trkseg><trkseg><trkpt lat="45.3" lon="6.3"></trkpt></trkseg></trk>
</gpx>"#;

    #[test]
    fn reads_points_name_and_start_time() {
        let t = parse(GPX.as_bytes()).unwrap();
        assert_eq!(t.name.as_deref(), Some("Morning ride"));
        assert_eq!(t.points, vec![(45.1, 6.1), (45.2, 6.2), (45.3, 6.3)]);
        assert_eq!(
            t.started_at.unwrap().to_rfc3339(),
            "2026-11-01T07:00:00+00:00"
        );
    }

    #[test]
    fn garbage_is_a_bad_request() {
        assert!(matches!(
            parse("nope".as_bytes()),
            Err(AppError::BadRequest(_))
        ));
    }
}
