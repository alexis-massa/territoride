//! TerritoRide: the library behind the `territoride` binary (split out so integration tests can build the
//! router and run the game logic).

pub mod capture;
pub mod commands;
pub mod crypto;
pub mod decay;
pub mod geojson;
pub mod gpx_file;
pub mod grid;
pub mod http;
pub mod import;
pub mod osm;
pub mod polyline;
pub mod routes;
pub mod scheduler;
pub mod scoring;
pub mod settings;
pub mod strava;
pub mod sync;
pub mod users;

/// The app's migrations, embedded in the binary.
pub fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!()
}
