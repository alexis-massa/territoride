//! Import of an uploaded GPX file.

use axum::Json;
use axum::extract::{Multipart, State};
use pi_common::auth::Session;
use pi_common::{ApiResult, AppError};
use sqlx::PgPool;

use super::Outcome;
use crate::gpx_file;

pub(super) const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Import an uploaded GPX file as an activity of the player.
pub(super) async fn import_activity(
    State(pool): State<PgPool>,
    session: Session,
    mut form: Multipart,
) -> ApiResult<Json<Outcome>> {
    let mut file = None;
    while let Some(field) = form
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("gpx_file") {
            let name = field.file_name().unwrap_or_default().to_owned();
            file = Some((
                name,
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?,
            ));
        }
    }
    let Some((file_name, bytes)) = file.filter(|(_, b)| !b.is_empty()) else {
        return Ok(Outcome::failed("No file selected."));
    };
    let Ok(track) = gpx_file::parse(bytes.as_ref()) else {
        return Ok(Outcome::failed("Couldn't read that file as GPX."));
    };
    if track.points.len() < 2 {
        return Ok(Outcome::failed("GPX file has fewer than 2 track points."));
    }
    let Some(recorded_at) = track.started_at else {
        return Ok(Outcome::failed("GPX file is missing activity date/time."));
    };
    let name = track.name.unwrap_or_else(|| {
        if file_name.is_empty() {
            "Untitled activity".into()
        } else {
            file_name
        }
    });
    let (cells, pois) =
        gpx_file::store(&pool, session.user_id, &name, recorded_at, &track.points).await?;
    Ok(Outcome::ok(format!(
        "Imported {name:?}: captured {cells} cells, {pois} passes."
    )))
}
