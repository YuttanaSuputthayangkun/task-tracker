//! Task Tracker: a REST API for personal task lists.
//!
//! Why the code is split into a library (this file) and a tiny binary
//! (main.rs): the integration tests in `tests/` are separate crates that can
//! only use what a *library* exports. With everything in the library, the
//! tests build exactly the same `Router` the real server runs, just pointed at
//! a throwaway in-memory database.
//!
//! The layers, outermost first (see GUIDE.md for diagrams):
//!
//! | Module          | Job                                              | Knows about HTTP? | Knows about SQL? |
//! |-----------------|--------------------------------------------------|-------------------|------------------|
//! | `routes`        | URL → handler, status codes, response shapes     | yes               | no               |
//! | `extractors`    | parse requests, authenticate the caller          | yes               | no               |
//! | `services`      | business rules                                   | no                | no               |
//! | `repositories`  | SQL queries                                      | no                | yes              |
//! | `models`        | data types shared by all layers                  | no                | no               |
//! | `error`         | one error type → one consistent error response   | yes               | no               |
//!
//! Dependencies only point *downwards* through that table: routes call
//! services, services call repositories, never the other way round.

pub mod config;
pub mod db;
pub mod error;
pub mod extractors;
pub mod models;
pub mod repositories;
pub mod routes;
pub mod services;
pub mod state;

use std::time::Duration;

use axum::{
    Router,
    http::{Method, StatusCode, header},
};
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::state::AppState;

/// Builds the complete application: routes plus the middleware that wraps
/// every request. Used by `main` and by the integration tests.
pub fn build_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        // Only the origins listed in config. An empty list allows no
        // cross-origin browser requests at all. (curl, Postman and other
        // servers aren't affected by CORS; it's a browser protection.)
        .allow_origin(state.config.cors_allowed_origins.clone())
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    routes::router(state)
        // Layers wrap from the inside out: the *last* `.layer()` is the
        // outermost and sees each request first. Tracing goes outermost so
        // the latency it logs includes everything, even time spent in the
        // other middleware.
        .layer(cors)
        // A request that takes more than 10s is answered with 408 instead of
        // tying up resources indefinitely.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(10),
        ))
        .layer(TraceLayer::new_for_http())
}
