//! Startup: read config, open the database, run migrations, serve.
//!
//! All the real code is in the library (lib.rs). This file only wires things
//! together, so it uses `anyhow`: if anything here fails there's nothing to
//! handle, only an error to print before exiting.

use std::sync::Arc;

use anyhow::Context;
use task_tracker::{build_app, config::Config, db, state::AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main] // Starts the tokio runtime and runs main() inside it.
async fn main() -> anyhow::Result<()> {
    // Logging first, so that everything after it can log.
    tracing_subscriber::fmt()
        .with_env_filter(
            // RUST_LOG controls verbosity per crate, with a sensible default.
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("task_tracker=debug,tower_http=info")),
        )
        .init();

    // Fail fast: a missing setting stops the server here, not on the first
    // request that needs it.
    let config = Config::from_env()?;

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    let port = config.port;
    let state = AppState {
        db: pool,
        config: Arc::new(config),
    };
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("failed to bind port {port}"))?;
    // We listen on IPv4 only (0.0.0.0). On Windows, "localhost" tries IPv6
    // (::1) first and waits ~2s before falling back to IPv4, so print the
    // address that's fast to use.
    tracing::info!("listening on http://127.0.0.1:{port}");

    // Graceful shutdown: on Ctrl+C, stop accepting new connections but let
    // requests already in progress finish, instead of cutting them off.
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("shutting down");
        })
        .await
        .context("server error")
}
