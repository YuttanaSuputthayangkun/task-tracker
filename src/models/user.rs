use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// A row of the `users` table.
///
/// Deliberately does NOT derive `Serialize` (so it can't be returned as JSON
/// by mistake) or `Debug` (so `password_hash` can't end up in a log line via
/// `?user`). Clients get a `UserResponse` instead.
#[derive(Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Body of `POST /auth/signup`.
///
/// No `Debug` derive here either: it contains a plaintext password.
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)] // Reject typos like "emial" instead of ignoring them
pub struct SignupRequest {
    // 254 is the longest a valid email address can be.
    #[validate(email, length(max = 254))]
    pub email: String,
    // The max matters as much as the min: hashing a 10 MB "password" ties up
    // a CPU core, so an unbounded length is a denial-of-service vector.
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

/// Body of `POST /auth/login`.
///
/// Only length limits, no format checks: a malformed email simply won't match
/// any account, and gets the same 401 as any other failed login.
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    #[validate(length(max = 254))]
    pub email: String,
    #[validate(length(max = 128))]
    pub password: String,
}

/// What clients see of a user. No password hash.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            created_at: user.created_at,
        }
    }
}

/// Response of a successful login. The field names follow OAuth 2's token
/// response (RFC 6749), which many HTTP clients and tools already understand.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    /// Seconds until the token expires.
    pub expires_in: u64,
}
