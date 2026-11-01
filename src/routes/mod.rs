//! The HTTP layer: which URL goes to which handler.
//!
//! Handlers here are deliberately thin. Each one only:
//!   1. receives already-parsed input (extractors did the parsing),
//!   2. runs the DTO's validation rules,
//!   3. calls one service function,
//!   4. picks the status code and converts the result into a response DTO.
//!
//! No SQL and no business rules live here. If a handler grows past a dozen
//! lines, something belongs in a service instead.

mod auth;
mod tasks;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::{error::AppError, state::AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .merge(auth::router())
        .merge(tasks::router())
        // Unknown paths and unsupported methods get our JSON error shape too,
        // instead of axum's empty-bodied 404 / 405.
        .fallback(|| async { AppError::NotFound })
        .method_not_allowed_fallback(|| async { AppError::MethodNotAllowed })
        // Provides AppState to every handler and extractor above.
        .with_state(state)
}

/// A cheap "is the process up?" endpoint for load balancers and uptime checks.
/// It doesn't require authentication.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
