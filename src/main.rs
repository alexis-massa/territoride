use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pi_common::cli::{self, App, Common, Context};
use pi_common::server::Security;
use territoride::commands::{self, WebhookAction};
use territoride::osm::PoiType;
use territoride::{migrator, routes};

#[derive(Parser)]
#[command(
    version,
    about = "TerritoRide: turn your rides and hikes into conquered territory"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    Common(Common),
    /// Load a GPX file as an activity of a player and capture its territory and POIs.
    LoadGpx { gpx_path: PathBuf, username: String },
    /// Replay capture for every activity, oldest first (needed after POIs are added).
    Recapture,
    /// Return cells and POIs past the decay threshold to unowned (the server also does this daily).
    ReleaseExpired,
    /// Import named passes or summits from OpenStreetMap (Overpass), worldwide; run again to resume.
    ImportOsmPois { poi_type: PoiType },
    /// Load POIs from an NDJSON file made by scripts/extract_osm_pois.py.
    ImportOsmPoisFromFile { path: PathBuf },
    /// Sync recent Strava activities for every connected player.
    SyncAllStrava,
    /// Create, view or delete this app's Strava webhook subscription (Strava allows one).
    StravaWebhook {
        action: WebhookAction,
        /// Public HTTPS URL for `create`, e.g. https://example.org/accounts/strava/webhook/
        #[arg(long)]
        callback_url: Option<String>,
    },
    /// List the players.
    ListUsers,
    /// List activities, newest first.
    ListActivities {
        /// Only this player's.
        #[arg(long)]
        user: Option<String>,
    },
    /// Delete an activity; what it held is released and the others reclaim it.
    DeleteActivity { id: i64 },
    /// Delete a player and everything of theirs.
    DeleteUser { username: String },
    /// Print the ranking.
    Leaderboard,
}

#[tokio::main]
async fn main() -> ExitCode {
    pi_common::telemetry::init();
    let command = match Cli::parse().command {
        Command::Common(common) => {
            let app = App {
                migrator: migrator(),
                security: Security::with_csp(routes::CSP),
                build: routes::build,
            };
            return cli::run(common, app).await;
        }
        other => other,
    };
    cli::report(run(command).await)
}

async fn run(command: Command) -> Result<(), pi_common::BoxError> {
    // Strava commands do not touch the database.
    if let Command::StravaWebhook {
        action,
        callback_url,
    } = &command
    {
        return commands::strava_webhook(*action, callback_url.as_deref()).await;
    }
    let ctx = Context::from_env(migrator()).await?;
    match command {
        Command::LoadGpx { gpx_path, username } => {
            commands::load_gpx(&ctx, &gpx_path, &username).await
        }
        Command::Recapture => commands::recapture(&ctx).await,
        Command::ReleaseExpired => commands::release_expired(&ctx).await,
        Command::ImportOsmPois { poi_type } => commands::import_osm_pois(&ctx, poi_type).await,
        Command::ImportOsmPoisFromFile { path } => {
            commands::import_osm_pois_from_file(&ctx, &path).await
        }
        Command::SyncAllStrava => commands::sync_all_strava(&ctx).await,
        Command::ListUsers => commands::list_users(&ctx).await,
        Command::ListActivities { user } => commands::list_activities(&ctx, user.as_deref()).await,
        Command::DeleteActivity { id } => commands::delete_activity(&ctx, id).await,
        Command::DeleteUser { username } => commands::delete_user(&ctx, &username).await,
        Command::Leaderboard => Ok(commands::leaderboard(&ctx).await?),
        Command::Common(_) | Command::StravaWebhook { .. } => unreachable!("handled above"),
    }
}
