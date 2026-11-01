//! The single error type every handler returns, and the single place where
//! errors become HTTP responses.
//!
//! Why one central type: handlers, services and repositories can all use `?`,
//! and the mapping "which failure is which status code, and what does the
//! client get to see" lives here and nowhere else. Every error response in the
//! API therefore has the same JSON shape:
//!
//! ```json
//! { "error": "human-readable message", "details": { ...optional... } }
//! ```

use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ---- Expected errors: the client did something we can explain. ----
    /// HTTP 404: Also used when a task exists but belongs to someone else: telling
    /// the caller "that's not yours" (403) would confirm the ID exists.
    #[error("not found")]
    NotFound,

    /// HTTP 401: The same variant for "no token", "bad token", "unknown email" and
    /// "wrong password", so the response never reveals which one it was.
    #[error("invalid or missing credentials")]
    Unauthorized,

    /// HTTP 409: e.g. signing up with an email that's already registered.
    #[error("{0}")]
    Conflict(String),

    /// HTTP 400: The request couldn't be parsed at all: malformed JSON, wrong field
    /// types, unknown fields, a non-UUID in the path, a bad query string.
    #[error("{0}")]
    BadRequest(String),

    /// HTTP 422: The request parsed fine but the values break a rule (an empty
    /// title, a short password). `#[from]` makes `payload.validate()?` work.
    #[error("validation failed")]
    Validation(#[from] validator::ValidationErrors),

    /// HTTP 405: Known path, unsupported HTTP method (e.g. PUT /tasks).
    #[error("method not allowed")]
    MethodNotAllowed,

    // ---- Unexpected errors: something is broken on our side. ----
    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, json!({ "error": self.to_string() })),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                json!({ "error": self.to_string() }),
            ),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, json!({ "error": msg })),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, json!({ "error": msg })),
            AppError::Validation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                // ValidationErrors serializes as { "field": [ { "code": ..., ... } ] },
                // so a client can highlight every bad field at once.
                json!({ "error": self.to_string(), "details": errors }),
            ),
            AppError::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                json!({ "error": self.to_string() }),
            ),
            // Unexpected failures: log the full details for *us*, send a
            // generic message to the *client*. The real error may contain SQL,
            // table names or file paths, none of which a client should see.
            // This is the one place errors get logged, so a single failure
            // produces a single log line.
            AppError::Database(_) | AppError::Internal(_) => {
                tracing::error!(error = ?self, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "error": "internal server error" }),
                )
            }
        };
        (status, Json(body)).into_response()
    }
}

// axum's built-in extractors reject bad input with a *plain-text* response.
// These conversions let our wrapper extractors (see extractors.rs) turn those
// rejections into AppError, so even "your JSON is malformed" comes back in the
// same JSON error shape as everything else.
//
// The rule used throughout the API:
//   400 = we couldn't understand the request (shape / syntax / types)
//   422 = we understood it, but the values aren't acceptable

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}

impl From<QueryRejection> for AppError {
    fn from(rejection: QueryRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        AppError::BadRequest(rejection.body_text())
    }
}
