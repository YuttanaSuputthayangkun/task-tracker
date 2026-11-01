//! Signup, login, password hashing and JWTs.

use std::sync::LazyLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    config::Config,
    error::AppError,
    models::user::{LoginRequest, SignupRequest, TokenResponse, User},
    repositories::users as user_repo,
};

// ---------------------------------------------------------------------------
// Use cases
// ---------------------------------------------------------------------------

#[tracing::instrument(skip_all)] // skip_all: the payload contains a password
pub async fn signup(db: &SqlitePool, payload: SignupRequest) -> Result<User, AppError> {
    let user = User {
        id: Uuid::now_v7(),
        email: normalize_email(&payload.email),
        password_hash: hash_password(payload.password).await?,
        created_at: Utc::now(),
    };

    // We don't check "is this email taken?" first and then insert: two
    // simultaneous signups could both pass the check. Instead we just insert
    // and let the UNIQUE constraint decide, then translate its error.
    match user_repo::insert(db, &user).await {
        Ok(()) => {}
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            // This does reveal that the email has an account. A signup form
            // can't avoid that without email verification, which is out of
            // scope here. Login, though, never reveals it.
            return Err(AppError::Conflict("email is already registered".into()));
        }
        Err(e) => return Err(e.into()),
    }

    tracing::info!(user_id = %user.id, "user signed up"); // No email, no password
    Ok(user)
}

#[tracing::instrument(skip_all)]
pub async fn login(
    db: &SqlitePool,
    config: &Config,
    payload: LoginRequest,
) -> Result<TokenResponse, AppError> {
    let user = user_repo::find_by_email(db, &normalize_email(&payload.email)).await?;

    // Always run a full password check, even when the email is unknown (we
    // then check against a dummy hash). If we returned early instead, unknown
    // emails would get a response tens of milliseconds faster than wrong
    // passwords, and an attacker could time responses to find out which
    // emails have accounts.
    let stored_hash = user.as_ref().map(|u| u.password_hash.clone());
    let password_ok = verify_password(payload.password, stored_hash).await?;

    // One error for both failure cases, so the response itself doesn't reveal
    // whether the email exists either.
    let user = match user {
        Some(user) if password_ok => user,
        _ => return Err(AppError::Unauthorized),
    };

    tracing::info!(user_id = %user.id, "user logged in");
    Ok(TokenResponse {
        access_token: issue_token(user.id, config)?,
        token_type: "Bearer",
        expires_in: config.jwt_ttl.as_secs(),
    })
}

pub async fn current_user(db: &SqlitePool, user_id: Uuid) -> Result<User, AppError> {
    // A valid token for a user who no longer exists is treated as "not logged
    // in" rather than "not found".
    user_repo::find_by_id(db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)
}

/// Emails are compared case-insensitively in practice ("Alice@Example.com" and
/// "alice@example.com" reach the same inbox), so store and look them up in
/// one canonical form.
fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Passwords
// ---------------------------------------------------------------------------

/// A hash of a throwaway password, computed on first use. Login verifies
/// against it when the email is unknown, so that path costs as much time as a
/// real check (see `login`).
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(b"dummy password for timing equalization", &salt)
        .expect("hashing a constant password cannot fail")
        .to_string()
});

async fn hash_password(password: String) -> Result<String, AppError> {
    // Argon2 is deliberately slow and memory-hungry (that's what makes
    // brute-forcing stolen hashes expensive). Tens of milliseconds of pure CPU
    // would stall one of tokio's few async worker threads, and every other
    // request scheduled on it, so it runs on tokio's blocking thread pool.
    let hash = tokio::task::spawn_blocking(move || {
        // A random salt per user: identical passwords get different hashes.
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            // The PHC string ("$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>")
            // records the algorithm and its parameters. We can raise the cost
            // later without breaking existing hashes.
            .map(|hash| hash.to_string())
            .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(hash)
}

/// Checks `password` against `stored_hash`, or against `DUMMY_HASH` if there is
/// no stored hash (unknown email). In the dummy case the result is always false.
async fn verify_password(password: String, stored_hash: Option<String>) -> Result<bool, AppError> {
    let ok = tokio::task::spawn_blocking(move || {
        let is_real = stored_hash.is_some();
        let hash_str = stored_hash.as_deref().unwrap_or(DUMMY_HASH.as_str());
        let parsed = PasswordHash::new(hash_str)
            .map_err(|e| anyhow::anyhow!("stored password hash is malformed: {e}"))?;
        let matches = Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        Ok::<_, anyhow::Error>(is_real && matches)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(ok)
}

// ---------------------------------------------------------------------------
// Tokens (JWT)
// ---------------------------------------------------------------------------

/// What we put inside a token. JWTs are signed, not encrypted: anyone can
/// base64-decode one and read this, so it must never contain secrets. A user
/// id and an expiry time are fine.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// "Subject": who the token is about.
    pub sub: Uuid,
    /// "Issued at", as a Unix timestamp.
    pub iat: usize,
    /// "Expiration time". `decode` rejects the token once this has passed,
    /// which caps how long a leaked token stays useful.
    pub exp: usize,
}

pub fn issue_token(user_id: Uuid, config: &Config) -> Result<String, AppError> {
    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id,
        iat: now,
        exp: now + config.jwt_ttl.as_secs() as usize,
    };
    // Header::default() is HS256: an HMAC signature using our secret. Only a
    // holder of the secret can produce a signature that verifies.
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(e.into()))
}

pub fn verify_token(token: &str, config: &Config) -> Result<Claims, AppError> {
    // Validation::default() pins the algorithm to HS256 and requires a valid
    // `exp`. Pinning the algorithm matters: it stops a forged token from
    // claiming a different (or no) algorithm to sidestep the signature check.
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    // Expired, tampered and garbage tokens all get the same 401. The reason
    // goes to the debug log only.
    .map_err(|e| {
        tracing::debug!(error = %e, "rejected token");
        AppError::Unauthorized
    })
}
