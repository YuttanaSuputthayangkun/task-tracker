//! Application configuration, read once from the environment at startup.
//!
//! Why a typed struct instead of calling `std::env::var` wherever a value is
//! needed: every setting the app depends on is listed in one place, each one
//! is parsed and checked exactly once, and a missing or malformed value stops
//! the server at startup with a clear message, instead of failing on the first
//! request that happens to need it (possibly hours later, in production).

use std::time::Duration;

use anyhow::{Context, bail};
use axum::http::HeaderValue;

/// Deliberately *not* `#[derive(Debug)]`: that would make it trivially easy to
/// log `jwt_secret` by accident with `tracing::debug!(?config)`.
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_ttl: Duration,
    pub port: u16,
    pub cors_allowed_origins: Vec<HeaderValue>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // The .env file is optional (production usually has none), and real
        // environment variables always win over values in .env.
        dotenvy::dotenv().ok();

        let jwt_secret = std::env::var("JWT_SECRET").context("JWT_SECRET must be set")?;
        // No default for secrets, ever. And refuse obviously weak ones: a
        // short secret can be brute-forced offline from any token we issue.
        if jwt_secret.len() < 32 {
            bail!("JWT_SECRET must be at least 32 characters long");
        }

        let jwt_ttl_minutes: u64 = std::env::var("JWT_TTL_MINUTES")
            .unwrap_or_else(|_| "60".into())
            .parse()
            .context("JWT_TTL_MINUTES must be a whole number of minutes")?;

        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(|origin| {
                origin
                    .parse::<HeaderValue>()
                    .with_context(|| format!("invalid origin in CORS_ALLOWED_ORIGINS: {origin}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            jwt_secret,
            jwt_ttl: Duration::from_secs(jwt_ttl_minutes * 60),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .context("PORT must be a number")?,
            cors_allowed_origins,
        })
    }
}
