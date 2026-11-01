//! Custom axum extractors: the types that appear as handler arguments.
//!
//! An extractor runs *before* the handler. If it fails, the handler never
//! runs and the extractor's rejection becomes the response. That's what makes
//! authentication impossible to forget: a handler that takes `user: AuthUser`
//! cannot execute without a valid token.

use axum::{
    extract::{FromRequest, FromRequestParts},
    http::{header, request::Parts},
};
use uuid::Uuid;

use crate::{error::AppError, services::auth::verify_token, state::AppState};

/// Drop-in replacements for axum's `Json`, `Query` and `Path` that work
/// exactly the same, except that a parse failure is returned as an `AppError`
/// (our JSON error shape) instead of axum's default plain-text response.
///
/// `via(axum::Json)` says "run axum's Json extractor"; `rejection(AppError)`
/// says "convert its failure with `From<JsonRejection> for AppError`".
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct AppJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(AppError))]
pub struct AppQuery<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct AppPath<T>(pub T);

/// The authenticated caller. Put `user: AuthUser` in a handler's arguments and
/// that endpoint requires a valid `Authorization: Bearer <token>` header.
pub struct AuthUser {
    pub id: Uuid,
}

// Implementing FromRequestParts (rather than FromRequest) means this extractor
// only reads headers and doesn't consume the body. It can sit anywhere in the
// argument list, alongside a body extractor like AppJson.
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        // The signature check proves *who* the caller is. *What* they may
        // access is enforced separately, by `WHERE user_id = ?` in every task
        // query.
        let claims = verify_token(token, &state.config)?;
        Ok(AuthUser { id: claims.sub })
    }
}
