//! Opening the database and running migrations.
//!
//! This is infrastructure rather than one of the request layers: it runs once
//! at startup (and once per integration test), and its output, a connection
//! pool, is what the repositories use.

use std::{str::FromStr, time::Duration};

use anyhow::Context;
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

/// The files in ./migrations, embedded into the binary at compile time. The
/// server therefore always carries the exact schema it was built against, and
/// a deployment doesn't need the .sql files next to the executable.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Opens a pool for a file-backed database (what `cargo run` uses).
pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)
        .context("DATABASE_URL is not a valid SQLite URL")?
        .create_if_missing(true)
        // SQLite ignores FOREIGN KEY / ON DELETE CASCADE unless this pragma is
        // switched on, separately for every connection.
        .foreign_keys(true)
        // Write-Ahead Logging lets readers keep reading while one connection
        // writes. Without it, any write blocks every read.
        .journal_mode(SqliteJournalMode::Wal)
        // SQLite allows one writer at a time. If a second write arrives, wait
        // up to 5s for the lock instead of failing immediately with "database
        // is locked".
        .busy_timeout(Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("failed to open the SQLite database")
}

/// Opens a private, empty, in-memory database. Used by the integration tests,
/// so each test starts from a clean slate and never touches tasks.db.
pub async fn connect_in_memory() -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);

    // Each SQLite connection to ":memory:" gets its *own* separate database,
    // and that database is destroyed when the connection closes. So the pool
    // must hold exactly one connection, and must never close it for being
    // idle or old. Otherwise a test could create a user on one connection and
    // find an empty database on the next.
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .context("failed to open the in-memory database")
}

/// Applies any migrations that haven't run yet. sqlx records the ones that
/// have run in a `_sqlx_migrations` table, so calling this on every startup is
/// safe.
pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("failed to run database migrations")
}
