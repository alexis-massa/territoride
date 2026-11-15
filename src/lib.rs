//! TerritoRide: the library behind the `territoride` binary (split out so integration tests can build the
//! router and run the game logic).

pub mod capture;
pub mod decay;
pub mod grid;
pub mod import;
pub mod polyline;
pub mod scoring;

/// The app's migrations, embedded in the binary.
pub fn migrator() -> sqlx::migrate::Migrator {
    sqlx::migrate!()
}
