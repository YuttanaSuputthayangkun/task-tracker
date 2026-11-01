//! Shared helpers for the integration tests.
//!
//! Every test calls `spawn_app()` and gets its own server backed by its own
//! empty in-memory database. Tests therefore can't see each other's data and
//! can run in parallel without interfering.

// Each file in tests/ is compiled as a separate crate that includes this
// module, and not every file uses every helper.
#![allow(dead_code)]

use std::{sync::Arc, time::Duration};

use axum_test::TestServer;
use serde_json::{Value, json};
use task_tracker::{build_app, config::Config, db, state::AppState};

pub const TEST_SECRET: &str = "test-secret-that-is-at-least-32-characters-long";
pub const PASSWORD: &str = "correct horse battery staple";

pub fn test_config() -> Config {
    Config {
        database_url: "sqlite::memory:".into(),
        jwt_secret: TEST_SECRET.into(),
        jwt_ttl: Duration::from_secs(3600),
        port: 0,
        cors_allowed_origins: vec![],
    }
}

/// Builds the real application (same routes, same middleware as `cargo run`)
/// on a fresh, fully migrated in-memory database.
pub async fn spawn_app() -> TestServer {
    let pool = db::connect_in_memory().await.expect("open in-memory db");
    db::migrate(&pool).await.expect("run migrations");
    let state = AppState {
        db: pool,
        config: Arc::new(test_config()),
    };
    // TestServer calls the Router directly, in-process. No real network port
    // is opened, which keeps the tests fast and avoids port conflicts.
    TestServer::new(build_app(state))
}

/// Signs up `email` and logs in, returning a bearer token.
pub async fn register(server: &TestServer, email: &str) -> String {
    server
        .post("/auth/signup")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await
        .assert_status(axum::http::StatusCode::CREATED);

    let response = server
        .post("/auth/login")
        .json(&json!({ "email": email, "password": PASSWORD }))
        .await;
    response.assert_status_ok();
    response.json::<Value>()["access_token"]
        .as_str()
        .expect("login response has access_token")
        .to_string()
}

/// Creates a task from the given JSON body and returns the response body.
pub async fn create_task(server: &TestServer, token: &str, body: Value) -> Value {
    let response = server
        .post("/tasks")
        .authorization_bearer(token)
        .json(&body)
        .await;
    response.assert_status(axum::http::StatusCode::CREATED);
    response.json()
}
