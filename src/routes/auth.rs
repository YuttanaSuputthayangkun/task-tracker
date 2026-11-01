use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::{
    error::AppError,
    extractors::{AppJson, AuthUser},
    models::user::{LoginRequest, SignupRequest, TokenResponse, UserResponse},
    services::auth as auth_service,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
}

/// `POST /auth/signup`: creates an account and returns it (201). The client
/// then logs in to get a token.
async fn signup(
    State(state): State<AppState>,
    // The body extractor must come last: it consumes the request body.
    AppJson(payload): AppJson<SignupRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;
    let user = auth_service::signup(&state.db, payload).await?;
    Ok((StatusCode::CREATED, Json(user.into())))
}

/// `POST /auth/login`: exchanges email + password for a JWT.
async fn login(
    State(state): State<AppState>,
    AppJson(payload): AppJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    payload.validate()?;
    let token = auth_service::login(&state.db, &state.config, payload).await?;
    Ok(Json(token))
}

/// `GET /auth/me`: who am I? Handy for checking that a token works.
async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<UserResponse>, AppError> {
    let user = auth_service::current_user(&state.db, user.id).await?;
    Ok(Json(user.into()))
}
