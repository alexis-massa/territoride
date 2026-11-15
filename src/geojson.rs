//! The map's data: territory, routes and POIs inside the viewport, as GeoJSON.
//!
//! `mine=1` restricts a layer to the logged-in player (403 without a login, rather than quietly serving
//! everyone's data to someone who asked for their own).

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, SecondsFormat, Utc};
use pi_common::auth::Session;
use pi_common::{ApiResult, AppError};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::decay::{current_value, elapsed_days};
use crate::grid;
use crate::scoring::{color_of, player_colors};

/// Route simplification tolerance as a fraction of the viewport width: sub-pixel detail is dropped here
/// instead of being shipped to the browser and discarded there.
const ROUTE_SIMPLIFY_DIVISOR: f64 = 1500.0;
const BBOX_ERROR: &str = "bbox must be min_lon,min_lat,max_lon,max_lat";

/// The query string of the three layers.
#[derive(Debug, Deserialize)]
pub struct MapQuery {
    bbox: Option<String>,
    mine: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl Bbox {
    /// `min_lon,min_lat,max_lon,max_lat`: four finite numbers.
    pub fn parse(text: Option<&str>) -> ApiResult<Self> {
        let bad = || AppError::BadRequest(BBOX_ERROR.into());
        let values: Vec<f64> = text
            .ok_or_else(bad)?
            .split(',')
            .map(|v| v.trim().parse::<f64>().ok().filter(|f| f.is_finite()))
            .collect::<Option<_>>()
            .ok_or_else(bad)?;
        let [min_lon, min_lat, max_lon, max_lat] = values[..] else {
            return Err(bad());
        };
        Ok(Self {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
        })
    }
}

/// The player a `mine=1` request is about, `None` for the shared view.
fn mine(query: &MapQuery, session: Option<&Session>) -> ApiResult<Option<i64>> {
    if query.mine.as_deref() != Some("1") {
        return Ok(None);
    }
    session
        .map(|s| Some(s.user_id))
        .ok_or_else(|| AppError::Forbidden("Log in to see only your own activity".into()))
}

fn collection(features: Vec<Value>) -> Json<Value> {
    Json(json!({"type": "FeatureCollection", "features": features}))
}

/// Python's `datetime.isoformat()` for a UTC time.
fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

fn cell_feature(
    cell_id: &str,
    owner: &str,
    color: &str,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Option<Value> {
    Some(json!({
        "type": "Feature",
        "geometry": {"type": "Polygon", "coordinates": [grid::boundary_of(cell_id)?]},
        "properties": {
            "owner": owner,
            "color": color,
            "captured_at": iso(at),
            "age_days": elapsed_days(at, now),
            "remaining": current_value(1.0, Some(at), now),
        },
    }))
}

/// Captured cells whose centre is in the viewport (with `mine=1`: every cell the player rode through recently,
/// including ones since taken from them).
pub async fn territory(
    State(pool): State<PgPool>,
    Query(query): Query<MapQuery>,
    session: Option<Session>,
) -> ApiResult<Json<Value>> {
    let b = Bbox::parse(query.bbox.as_deref())?;
    let now = Utc::now();
    let colors = player_colors(&pool).await?;
    let features = if let Some(user_id) = mine(&query, session.as_ref())? {
        let name = crate::users::username(&pool, user_id).await?;
        let color = color_of(&colors, &name);
        sqlx::query!(
            "SELECT cell_id, visited_at FROM game_cellvisit
             WHERE user_id = $1 AND ST_Within(centroid, ST_MakeEnvelope($2, $3, $4, $5, 4326))",
            user_id,
            b.min_lon,
            b.min_lat,
            b.max_lon,
            b.max_lat
        )
        .fetch_all(&pool)
        .await?
        .iter()
        .filter_map(|v| cell_feature(&v.cell_id, &name, &color, v.visited_at, now))
        .collect()
    } else {
        sqlx::query!(
            r#"SELECT c.cell_id, u.username, c.captured_at AS "captured_at!"
               FROM game_territorycell c JOIN accounts_user u ON u.id = c.owner_id
               WHERE ST_Within(c.centroid, ST_MakeEnvelope($1, $2, $3, $4, 4326))"#,
            b.min_lon,
            b.min_lat,
            b.max_lon,
            b.max_lat
        )
        .fetch_all(&pool)
        .await?
        .iter()
        .filter_map(|c| {
            cell_feature(
                &c.cell_id,
                &c.username,
                &color_of(&colors, &c.username),
                c.captured_at,
                now,
            )
        })
        .collect()
    };
    Ok(collection(features))
}

/// Tracks crossing the viewport: those that captured a cell, or with `mine=1` every one of the player's.
pub async fn routes(
    State(pool): State<PgPool>,
    Query(query): Query<MapQuery>,
    session: Option<Session>,
) -> ApiResult<Json<Value>> {
    let b = Bbox::parse(query.bbox.as_deref())?;
    let mine = mine(&query, session.as_ref())?;
    let tolerance = (b.max_lon - b.min_lon) / ROUTE_SIMPLIFY_DIVISOR;
    let now = Utc::now();
    let colors = player_colors(&pool).await?;
    let rows = sqlx::query!(
        r#"SELECT a.name, u.username AS owner, a.recorded_at,
                  ST_AsGeoJSON(ST_SimplifyPreserveTopology(a.track, $5), 15)::jsonb -> 'coordinates' AS "coordinates!: Value"
           FROM game_activity a JOIN accounts_user u ON u.id = a.user_id
           WHERE ST_Intersects(a.track, ST_MakeEnvelope($1, $2, $3, $4, 4326))
             AND CASE WHEN $6::bigint IS NULL
                      THEN EXISTS (SELECT 1 FROM game_territorycell c WHERE c.captured_by_id = a.id)
                      ELSE a.user_id = $6 END"#,
        b.min_lon,
        b.min_lat,
        b.max_lon,
        b.max_lat,
        tolerance,
        mine
    )
    .fetch_all(&pool)
    .await?;
    Ok(collection(
        rows.into_iter()
            .map(|a| {
                json!({
                    "type": "Feature",
                    "geometry": {"type": "LineString", "coordinates": a.coordinates},
                    "properties": {
                        "name": a.name,
                        "owner": a.owner,
                        "color": color_of(&colors, &a.owner),
                        "age_days": elapsed_days(a.recorded_at, now),
                    },
                })
            })
            .collect(),
    ))
}

/// Passes and summits in the viewport; with `mine=1` the ones held by others show as unclaimed.
pub async fn pois(
    State(pool): State<PgPool>,
    Query(query): Query<MapQuery>,
    session: Option<Session>,
) -> ApiResult<Json<Value>> {
    let b = Bbox::parse(query.bbox.as_deref())?;
    let only = mine(&query, session.as_ref())?;
    let now = Utc::now();
    let colors = player_colors(&pool).await?;
    let rows = sqlx::query!(
        r#"SELECT p.name, p.poi_type, p.altitude_m, ST_X(p.location) AS "x!", ST_Y(p.location) AS "y!",
                  p.owner_id, u.username AS "owner?", p.claimed_at
           FROM game_poi p LEFT JOIN accounts_user u ON u.id = p.owner_id
           WHERE ST_Within(p.location, ST_MakeEnvelope($1, $2, $3, $4, 4326))"#,
        b.min_lon,
        b.min_lat,
        b.max_lon,
        b.max_lat
    )
    .fetch_all(&pool)
    .await?;
    Ok(collection(
        rows.into_iter()
            .map(|p| {
                let mut properties = json!({
                    "name": p.name,
                    "poi_type": p.poi_type,
                    "poi_type_label": crate::scoring::poi_type_label(&p.poi_type),
                    "altitude_m": p.altitude_m,
                });
                if let (Some(owner), Some(at)) = (&p.owner, p.claimed_at)
                    && only.is_none_or(|id| p.owner_id == Some(id))
                {
                    properties["owner"] = json!(owner);
                    properties["color"] = json!(color_of(&colors, owner));
                    properties["captured_at"] = json!(iso(at));
                    properties["age_days"] = json!(elapsed_days(at, now));
                    properties["remaining"] = json!(current_value(1.0, Some(at), now));
                }
                json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [p.x, p.y]}, "properties": properties})
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_needs_four_finite_numbers() {
        let ok = Bbox::parse(Some("5.5,45,6.5, 46.25")).unwrap();
        assert_eq!((ok.min_lon, ok.max_lat), (5.5, 46.25));
        for bad in [
            None,
            Some(""),
            Some("1,2,3"),
            Some("1,2,3,4,5"),
            Some("1,2,x,4"),
            Some("1,2,inf,4"),
            Some("1,2,NaN,4"),
        ] {
            assert!(
                matches!(Bbox::parse(bad), Err(AppError::BadRequest(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn mine_needs_a_login() {
        let q = |m: Option<&str>| MapQuery {
            bbox: None,
            mine: m.map(String::from),
        };
        assert_eq!(mine(&q(None), None).unwrap(), None);
        assert_eq!(mine(&q(Some("0")), None).unwrap(), None);
        assert!(matches!(
            mine(&q(Some("1")), None),
            Err(AppError::Forbidden(_))
        ));
    }
}
