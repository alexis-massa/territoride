//! Capture rules: which cells a track holds (pure functions here), and claiming them in the database.

use std::collections::HashSet;

use h3o::CellIndex;

use crate::grid::{self, LatLon};

/// Strava trims about 200 m at the start and end, so a loop can show up to about 400 m "open".
pub const LOOP_CLOSE_TOLERANCE_M: f64 = 500.0;
/// A pass or summit within this distance of a track is captured.
pub const POI_CAPTURE_RADIUS_M: f64 = 100.0;

/// Whether a track's endpoints are within [`LOOP_CLOSE_TOLERANCE_M`] of each other.
pub fn is_loop(points: &[LatLon]) -> bool {
    grid::distance_m(points[0], points[points.len() - 1]) <= LOOP_CLOSE_TOLERANCE_M
}

/// The cells a track passes through.
pub fn touched_cells(points: &[LatLon]) -> HashSet<CellIndex> {
    points.iter().map(|&p| grid::cell_at(p)).collect()
}

/// The cells a track captures: the touched ones, plus the enclosed ones if it is a loop.
pub fn captured_cells(points: &[LatLon]) -> HashSet<CellIndex> {
    let mut cells = touched_cells(points);
    if is_loop(points) {
        cells.extend(grid::cells_in_loop(points));
    }
    cells
}

// ---------- database ----------

use chrono::{DateTime, Utc};
use pi_common::ApiResult;
use sqlx::{PgConnection, PgPool};

/// What it takes to store a new activity.
pub struct NewActivity<'a> {
    pub user_id: i64,
    pub name: &'a str,
    /// `(lat, lng)` points; `None` for e.g. a swim with no location data.
    pub track: Option<&'a [LatLon]>,
    pub recorded_at: DateTime<Utc>,
    pub strava_activity_id: Option<i64>,
    pub sport_type: &'a str,
    pub distance_m: Option<i32>,
}

/// `LINESTRING(lng lat, …)` of a track; `f64` is written with its shortest exact representation.
fn wkt(points: &[LatLon]) -> String {
    let coords: Vec<String> = points
        .iter()
        .map(|(lat, lng)| format!("{lng} {lat}"))
        .collect();
    format!("LINESTRING({})", coords.join(", "))
}

/// Store an activity and return its id (nothing is captured yet).
pub async fn create_activity(conn: &mut PgConnection, a: &NewActivity<'_>) -> ApiResult<i64> {
    let track = a.track.map(wkt);
    Ok(sqlx::query_scalar!(
        "INSERT INTO game_activity (user_id, name, track, recorded_at, strava_activity_id, sport_type, distance_m)
         VALUES ($1, $2, ST_SetSRID(ST_GeomFromText($3), 4326), $4, $5, $6, $7) RETURNING id",
        a.user_id,
        a.name,
        track,
        a.recorded_at,
        a.strava_activity_id,
        a.sport_type,
        a.distance_m
    )
    .fetch_one(conn)
    .await?)
}

/// Remember the player's latest visit to each cell, whoever ends up holding it. Keyed on recording time like
/// claiming: a late-synced older ride never replaces a more recent visit.
async fn record_visits(
    conn: &mut PgConnection,
    user_id: i64,
    activity_id: i64,
    at: DateTime<Utc>,
    cells: &[CellIndex],
) -> ApiResult<()> {
    let (ids, xs, ys) = cell_columns(cells);
    sqlx::query!(
        "INSERT INTO game_cellvisit (user_id, cell_id, centroid, activity_id, visited_at)
         SELECT $1, c.id, ST_SetSRID(ST_MakePoint(c.x, c.y), 4326), $2, $3
         FROM unnest($4::text[], $5::float8[], $6::float8[]) AS c(id, x, y)
         ON CONFLICT ON CONSTRAINT unique_visit_per_user_cell DO NOTHING",
        user_id,
        activity_id,
        at,
        &ids,
        &xs,
        &ys
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE game_cellvisit SET activity_id = $2, visited_at = $3
         WHERE user_id = $1 AND cell_id = ANY($4) AND visited_at < $3",
        user_id,
        activity_id,
        at,
        &ids
    )
    .execute(conn)
    .await?;
    Ok(())
}

fn cell_columns(cells: &[CellIndex]) -> (Vec<String>, Vec<f64>, Vec<f64>) {
    let mut ids = Vec::with_capacity(cells.len());
    let (mut xs, mut ys) = (
        Vec::with_capacity(cells.len()),
        Vec::with_capacity(cells.len()),
    );
    for &c in cells {
        let (x, y) = grid::cell_centroid(c);
        ids.push(c.to_string());
        xs.push(x);
        ys.push(y);
    }
    (ids, xs, ys)
}

/// Claim cells for an activity, unless already held by a more recent capture: "last rider through wins" by
/// recording time, not import order. Returns how many cells changed hands.
async fn claim_cells(
    conn: &mut PgConnection,
    a: &ActivityRef,
    cells: &HashSet<CellIndex>,
) -> ApiResult<u64> {
    let cells: Vec<CellIndex> = cells.iter().copied().collect();
    record_visits(conn, a.user_id, a.id, a.recorded_at, &cells).await?;
    let (ids, xs, ys) = cell_columns(&cells);
    sqlx::query!(
        "INSERT INTO game_territorycell (cell_id, centroid)
         SELECT c.id, ST_SetSRID(ST_MakePoint(c.x, c.y), 4326)
         FROM unnest($1::text[], $2::float8[], $3::float8[]) AS c(id, x, y)
         ON CONFLICT (cell_id) DO NOTHING",
        &ids,
        &xs,
        &ys
    )
    .execute(&mut *conn)
    .await?;
    Ok(sqlx::query!(
        "UPDATE game_territorycell SET owner_id = $2, captured_by_id = $3, captured_at = $4
         WHERE cell_id = ANY($1) AND (owner_id IS NULL OR captured_at < $4)",
        &ids,
        a.user_id,
        a.id,
        a.recorded_at
    )
    .execute(conn)
    .await?
    .rows_affected())
}

/// The part of an activity the capture rules need.
#[derive(Debug, Clone, Copy)]
pub struct ActivityRef {
    pub id: i64,
    pub user_id: i64,
    pub recorded_at: DateTime<Utc>,
}

/// Claim the single cell at a point; returns 1 if claimed, 0 if held more recently.
pub async fn capture_point(
    conn: &mut PgConnection,
    a: &ActivityRef,
    point: LatLon,
) -> ApiResult<u64> {
    claim_cells(conn, a, &HashSet::from([grid::cell_at(point)])).await
}

/// Claim every cell a track captures; returns how many were actually claimed.
pub async fn capture_territory(
    conn: &mut PgConnection,
    a: &ActivityRef,
    track: &[LatLon],
) -> ApiResult<u64> {
    claim_cells(conn, a, &captured_cells(track)).await
}

/// Claim every pass or summit within [`POI_CAPTURE_RADIUS_M`] of the activity's track, unless held more recently.
pub async fn capture_pois(conn: &mut PgConnection, a: &ActivityRef) -> ApiResult<u64> {
    Ok(sqlx::query!(
        "UPDATE game_poi SET owner_id = $2, claimed_by_id = $1, claimed_at = $3
         WHERE (owner_id IS NULL OR claimed_at < $3)
           AND ST_DistanceSphere(location, (SELECT track FROM game_activity WHERE id = $1)) <= $4",
        a.id,
        a.user_id,
        a.recorded_at,
        POI_CAPTURE_RADIUS_M
    )
    .execute(conn)
    .await?
    .rows_affected())
}

/// Capture what an activity's track holds, as when it is imported: its cell(s), then the POIs near it.
/// A track of one repeated point holds that point's cell only. Returns `(cells, pois)` claimed.
pub async fn capture_activity(
    conn: &mut PgConnection,
    a: &ActivityRef,
    track: &[LatLon],
) -> ApiResult<(u64, u64)> {
    let first = track[0];
    let cells = if track.iter().all(|p| *p == first) {
        capture_point(conn, a, first).await?
    } else {
        capture_territory(conn, a, track).await?
    };
    Ok((cells, capture_pois(conn, a).await?))
}

/// Activities with a track, oldest first, with their points.
async fn tracked_activities(conn: &mut PgConnection) -> ApiResult<Vec<(ActivityRef, Vec<LatLon>)>> {
    let rows = sqlx::query!(
        r#"SELECT id, user_id, recorded_at,
                  ARRAY(SELECT ST_Y(p.geom) FROM ST_DumpPoints(track) p ORDER BY p.path) AS "lats!",
                  ARRAY(SELECT ST_X(p.geom) FROM ST_DumpPoints(track) p ORDER BY p.path) AS "lngs!"
           FROM game_activity WHERE track IS NOT NULL ORDER BY recorded_at, id"#
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let points = r.lats.into_iter().zip(r.lngs).collect();
            (
                ActivityRef {
                    id: r.id,
                    user_id: r.user_id,
                    recorded_at: r.recorded_at,
                },
                points,
            )
        })
        .collect())
}

/// Replay capture for every activity, oldest first, so the most recent through each cell and POI ends up
/// owning it. Needed after POIs are added or an activity is deleted. Returns `(cells, pois)` claimed.
pub async fn recapture_all(pool: &PgPool) -> ApiResult<(u64, u64)> {
    let mut tx = pool.begin().await?;
    let (mut cells, mut pois) = (0, 0);
    for (activity, track) in tracked_activities(&mut tx).await? {
        let (c, p) = capture_activity(&mut tx, &activity, &track).await?;
        cells += c;
        pois += p;
    }
    tx.commit().await?;
    Ok((cells, pois))
}

/// Release everything an activity holds, delete it (and its visits), then let the others reclaim.
pub async fn release_activity(pool: &PgPool, activity_id: i64) -> ApiResult<()> {
    let mut tx = pool.begin().await?;
    delete_activities(&mut tx, &[activity_id]).await?;
    tx.commit().await?;
    recapture_all(pool).await?;
    Ok(())
}

/// Delete activities with what hangs off them: cells and POIs they held become unowned and their visits go
/// (Django's SET_NULL and CASCADE, which this schema leaves to the application).
async fn delete_activities(conn: &mut PgConnection, ids: &[i64]) -> ApiResult<()> {
    sqlx::query!(
        "UPDATE game_territorycell SET owner_id = NULL, captured_by_id = NULL, captured_at = NULL
         WHERE captured_by_id = ANY($1)",
        ids
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE game_poi SET owner_id = NULL, claimed_by_id = NULL, claimed_at = NULL WHERE claimed_by_id = ANY($1)",
        ids
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM game_cellvisit WHERE activity_id = ANY($1)",
        ids
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("DELETE FROM game_activity WHERE id = ANY($1)", ids)
        .execute(conn)
        .await?;
    Ok(())
}

/// Return cells and POIs past the decay threshold to unowned and delete the activities past it. Returns how
/// many cells and POIs were released.
pub async fn release_expired(pool: &PgPool, now: DateTime<Utc>) -> ApiResult<u64> {
    let cutoff = now - chrono::Duration::days(crate::decay::DECAY_THRESHOLD_DAYS);
    let mut tx = pool.begin().await?;
    let cells = sqlx::query!(
        "UPDATE game_territorycell SET owner_id = NULL, captured_by_id = NULL, captured_at = NULL
         WHERE captured_at < $1",
        cutoff
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let pois = sqlx::query!(
        "UPDATE game_poi SET owner_id = NULL, claimed_by_id = NULL, claimed_at = NULL WHERE claimed_at < $1",
        cutoff
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let expired = sqlx::query_scalar!(
        "SELECT id FROM game_activity WHERE recorded_at < $1",
        cutoff
    )
    .fetch_all(&mut *tx)
    .await?;
    delete_activities(&mut tx, &expired).await?;
    tx.commit().await?;
    Ok(cells + pois)
}

/// Delete a player and everything of theirs (the Django rows too), then let the others reclaim what they held.
pub async fn delete_user(pool: &PgPool, user_id: i64) -> ApiResult<()> {
    let mut tx = pool.begin().await?;
    let activities =
        sqlx::query_scalar!("SELECT id FROM game_activity WHERE user_id = $1", user_id)
            .fetch_all(&mut *tx)
            .await?;
    delete_activities(&mut tx, &activities).await?;
    sqlx::query!("UPDATE game_territorycell SET owner_id = NULL, captured_by_id = NULL, captured_at = NULL WHERE owner_id = $1", user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("UPDATE game_poi SET owner_id = NULL, claimed_by_id = NULL, claimed_at = NULL WHERE owner_id = $1", user_id)
        .execute(&mut *tx)
        .await?;
    for table_delete in [
        sqlx::query!("DELETE FROM game_cellvisit WHERE user_id = $1", user_id),
        sqlx::query!(
            "DELETE FROM accounts_user_groups WHERE user_id = $1",
            user_id
        ),
        sqlx::query!(
            "DELETE FROM accounts_user_user_permissions WHERE user_id = $1",
            user_id
        ),
        sqlx::query!("DELETE FROM django_admin_log WHERE user_id = $1", user_id),
        sqlx::query!("DELETE FROM app_sessions WHERE user_id = $1", user_id),
        sqlx::query!("DELETE FROM accounts_user WHERE id = $1", user_id),
    ] {
        table_delete.execute(&mut *tx).await?;
    }
    tx.commit().await?;
    recapture_all(pool).await?;
    Ok(())
}
