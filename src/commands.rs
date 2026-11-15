//! The command-line side: what the Python version's management commands did, plus the few things its Django
//! admin was used for (looking at players and activities, deleting them).

use std::path::Path;

use chrono::Utc;
use pi_common::cli::Context;
use pi_common::{ApiResult, AppError, BoxError};

use crate::capture;
use crate::osm::{self, PoiType};
use crate::settings::Settings;
use crate::strava::Strava;
use crate::sync::{Services, SyncError};
use crate::{gpx_file, scoring, users};

/// Load a GPX file as an activity of a player.
pub async fn load_gpx(ctx: &Context, path: &Path, username: &str) -> Result<(), BoxError> {
    let user_id = users::id_by_username(&ctx.pool, username)
        .await
        .map_err(|_| format!("no user named {username:?}"))?;
    let (name, cells, pois) = gpx_file::load(&ctx.pool, user_id, path, Utc::now()).await?;
    println!("Loaded {name:?}: captured {cells} cells, {pois} POIs");
    Ok(())
}

/// Replay capture for every activity, oldest first (needed after POIs are added).
pub async fn recapture(ctx: &Context) -> Result<(), BoxError> {
    let (cells, pois) = capture::recapture_all(&ctx.pool).await?;
    let activities = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM game_activity"#)
        .fetch_one(&ctx.pool)
        .await?;
    println!("Recaptured {cells} cells and {pois} POIs across {activities} activities");
    Ok(())
}

/// Return cells and POIs past the decay threshold to unowned.
pub async fn release_expired(ctx: &Context) -> Result<(), BoxError> {
    let released = capture::release_expired(&ctx.pool, Utc::now()).await?;
    println!("Released {released} expired cells/POIs");
    Ok(())
}

pub async fn import_osm_pois(ctx: &Context, poi_type: PoiType) -> Result<(), BoxError> {
    let (created, fetched) = osm::Crawl::new(&ctx.pool, poi_type).run().await?;
    println!(
        "Done: {created} new POIs ({} already existed, {fetched} fetched total)",
        fetched - created
    );
    Ok(())
}

pub async fn import_osm_pois_from_file(ctx: &Context, path: &Path) -> Result<(), BoxError> {
    let (created, total) = osm::import_file(&ctx.pool, path).await?;
    println!(
        "Done: {created} new POIs ({} already existed, {total} total)",
        total - created
    );
    Ok(())
}

fn services(ctx: &Context) -> Result<Services, BoxError> {
    let settings = Settings::from_env()?;
    Ok(Services {
        pool: ctx.pool.clone(),
        cipher: settings.cipher,
        strava: Strava::new(&settings.strava_client_id, &settings.strava_client_secret),
    })
}

/// Sync the recent activities of every player who connected Strava.
pub async fn sync_all_strava(ctx: &Context) -> Result<(), BoxError> {
    let services = services(ctx)?;
    for (id, name) in users::connected(&ctx.pool).await? {
        match services.sync_player(id, Utc::now()).await {
            Ok(c) => println!(
                "{name}: imported {}, captured {} cells, {} passes",
                c.activities, c.cells, c.pois
            ),
            Err(SyncError::App(e)) => return Err(e.into()),
            Err(e) => println!("{name}: skipped, {e}"),
        }
    }
    Ok(())
}

/// What `strava-webhook` does.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum WebhookAction {
    Create,
    View,
    Delete,
}

/// Create, view or delete this app's Strava webhook subscription (Strava allows one).
pub async fn strava_webhook(
    action: WebhookAction,
    callback_url: Option<&str>,
) -> Result<(), BoxError> {
    let settings = Settings::from_env()?;
    let strava = Strava::new(&settings.strava_client_id, &settings.strava_client_secret);
    match action {
        WebhookAction::Create => {
            let url = callback_url.ok_or("--callback-url is required to create a subscription")?;
            println!(
                "Created: {}",
                strava
                    .create_subscription(url, &settings.webhook_verify_token)
                    .await?
            );
        }
        WebhookAction::View => println!("{:?}", strava.subscriptions().await?),
        WebhookAction::Delete => {
            for subscription in strava.subscriptions().await? {
                let id = subscription["id"]
                    .as_i64()
                    .ok_or("a subscription without an id")?;
                strava.delete_subscription(id).await?;
                println!("Deleted subscription {id}");
            }
        }
    }
    Ok(())
}

/// The players: id, name, Strava athlete id, whether Strava is connected.
pub async fn list_users(ctx: &Context) -> Result<(), BoxError> {
    for (id, name, athlete, connected) in users::all(&ctx.pool).await? {
        let athlete = athlete.map_or_else(|| "-".into(), |a| a.to_string());
        println!(
            "{id}\t{name}\tathlete {athlete}\t{}",
            if connected {
                "connected"
            } else {
                "not connected"
            }
        );
    }
    Ok(())
}

/// Activities, newest first, optionally of one player.
pub async fn list_activities(ctx: &Context, username: Option<&str>) -> Result<(), BoxError> {
    let user_id = match username {
        Some(name) => Some(
            users::id_by_username(&ctx.pool, name)
                .await
                .map_err(|_| format!("no user named {name:?}"))?,
        ),
        None => None,
    };
    let rows = sqlx::query!(
        r#"SELECT a.id, u.username, a.name, a.sport_type, a.recorded_at, a.distance_m, a.track IS NOT NULL AS "tracked!"
           FROM game_activity a JOIN accounts_user u ON u.id = a.user_id
           WHERE $1::bigint IS NULL OR a.user_id = $1 ORDER BY a.recorded_at DESC, a.id DESC"#,
        user_id
    )
    .fetch_all(&ctx.pool)
    .await?;
    for r in rows {
        let km = r
            .distance_m
            .map_or_else(String::new, |m| format!(" {:.1} km", f64::from(m) / 1000.0));
        let sport = if r.sport_type.is_empty() {
            "Ride"
        } else {
            &r.sport_type
        };
        println!(
            "{}\t{}\t{}\t{sport}{km}{}\t{:?}",
            r.id,
            r.recorded_at.format("%Y-%m-%d %H:%M"),
            r.username,
            if r.tracked { "" } else { " (no track)" },
            r.name
        );
    }
    Ok(())
}

/// Delete an activity, releasing what it held; others reclaim.
pub async fn delete_activity(ctx: &Context, id: i64) -> Result<(), BoxError> {
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM game_activity WHERE id = $1) AS "e!""#,
        id
    )
    .fetch_one(&ctx.pool)
    .await?;
    if !exists {
        return Err(format!("no activity {id}").into());
    }
    capture::release_activity(&ctx.pool, id).await?;
    println!("Deleted activity {id}; the others reclaimed what it held");
    Ok(())
}

/// Delete a player and everything of theirs, revoking Strava access when connected.
pub async fn delete_user(ctx: &Context, username: &str) -> Result<(), BoxError> {
    let id = users::id_by_username(&ctx.pool, username)
        .await
        .map_err(|_: AppError| format!("no user named {username:?}"))?;
    services(ctx)?.delete_account(id).await?;
    println!("Deleted {username}");
    Ok(())
}

/// The ranking, as the leaderboard page shows it.
pub async fn leaderboard(ctx: &Context) -> ApiResult<()> {
    for (i, row) in scoring::leaderboard(&ctx.pool, Utc::now(), false)
        .await?
        .iter()
        .enumerate()
    {
        println!(
            "{}\t{}\t{} points ({} cells, {} passes)",
            i + 1,
            row.username,
            row.score.total,
            row.cell_count,
            row.poi_count
        );
    }
    Ok(())
}
