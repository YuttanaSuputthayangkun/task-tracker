//! Signup, login and token handling.

mod common;

use axum::http::StatusCode;
use chrono::Utc;
use common::{PASSWORD, TEST_SECRET, register, spawn_app};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use task_tracker::services::auth::Claims;
use uuid::Uuid;

#[tokio::test]
async fn signup_returns_the_user_without_the_password_hash() {
    let server = spawn_app().await;

    let response = server
        .post("/auth/signup")
        .json(&json!({ "email": "Alice@Example.com", "password": PASSWORD }))
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    // Emails are normalized to lowercase.
    assert_eq!(body["email"], "alice@example.com");
    assert!(body["id"].is_string());
    assert!(body.get("password_hash").is_none());
    assert!(body.get("password").is_none());
}

#[tokio::test]
async fn signup_with_an_existing_email_is_a_conflict_regardless_of_case() {
    let server = spawn_app().await;
    register(&server, "alice@example.com").await;

    let response = server
        .post("/auth/signup")
        .json(&json!({ "email": "ALICE@example.com", "password": PASSWORD }))
        .await;

    response.assert_status(StatusCode::CONFLICT);
    assert_eq!(
        response.json::<Value>()["error"],
        "email is already registered"
    );
}

#[tokio::test]
async fn signup_reports_every_invalid_field_at_once() {
    let server = spawn_app().await;

    let response = server
        .post("/auth/signup")
        .json(&json!({ "email": "not-an-email", "password": "short" }))
        .await;

    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = response.json();
    assert_eq!(body["error"], "validation failed");
    assert!(body["details"]["email"].is_array());
    assert!(body["details"]["password"].is_array());
}

#[tokio::test]
async fn signup_rejects_unknown_fields() {
    let server = spawn_app().await;

    // Mass assignment attempt: the client tries to set a field it shouldn't.
    let response = server
        .post("/auth/signup")
        .json(&json!({ "email": "a@example.com", "password": PASSWORD, "is_admin": true }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_json_gets_a_json_error_response() {
    let server = spawn_app().await;

    let response = server
        .post("/auth/signup")
        .content_type("application/json")
        .bytes("{ this is not json".into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
    // Even parse failures use the standard error shape.
    assert!(response.json::<Value>()["error"].is_string());
}

#[tokio::test]
async fn login_returns_a_token_that_works() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let response = server.get("/auth/me").authorization_bearer(&token).await;

    response.assert_status_ok();
    assert_eq!(response.json::<Value>()["email"], "alice@example.com");
}

#[tokio::test]
async fn login_failures_are_indistinguishable() {
    let server = spawn_app().await;
    register(&server, "alice@example.com").await;

    let wrong_password = server
        .post("/auth/login")
        .json(&json!({ "email": "alice@example.com", "password": "wrong password" }))
        .await;
    let unknown_email = server
        .post("/auth/login")
        .json(&json!({ "email": "nobody@example.com", "password": PASSWORD }))
        .await;

    wrong_password.assert_status(StatusCode::UNAUTHORIZED);
    unknown_email.assert_status(StatusCode::UNAUTHORIZED);
    // Same body too, so the response doesn't reveal which emails exist.
    assert_eq!(wrong_password.text(), unknown_email.text());
}

#[tokio::test]
async fn protected_routes_reject_missing_and_invalid_tokens() {
    let server = spawn_app().await;

    server
        .get("/auth/me")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    server
        .get("/auth/me")
        .authorization_bearer("not.a.jwt")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    // Right scheme name required.
    server
        .get("/auth/me")
        .add_header("authorization", "Basic dXNlcjpwYXNz")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

/// Builds a token by hand, bypassing the app's own `issue_token`.
fn forge_token(user_id: Uuid, secret: &str, exp_offset_secs: i64) -> String {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id,
        iat: now as usize,
        exp: (now + exp_offset_secs) as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

#[tokio::test]
async fn expired_tokens_are_rejected() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    let me: Value = server
        .get("/auth/me")
        .authorization_bearer(&token)
        .await
        .json();
    let user_id: Uuid = me["id"].as_str().unwrap().parse().unwrap();

    // Expired two hours ago, well beyond jsonwebtoken's default 60s leeway.
    let expired = forge_token(user_id, TEST_SECRET, -7200);

    server
        .get("/auth/me")
        .authorization_bearer(&expired)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tokens_signed_with_another_secret_are_rejected() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    let me: Value = server
        .get("/auth/me")
        .authorization_bearer(&token)
        .await
        .json();
    let user_id: Uuid = me["id"].as_str().unwrap().parse().unwrap();

    // Valid shape, valid expiry, real user id, but the attacker doesn't know
    // our secret.
    let forged = forge_token(
        user_id,
        "an-attacker-guessed-secret-of-sufficient-length",
        3600,
    );

    server
        .get("/auth/me")
        .authorization_bearer(&forged)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}
