//! Task CRUD and per-user authorization.

mod common;

use axum::http::StatusCode;
use common::{create_task, register, spawn_app};
use serde_json::{Value, json};

#[tokio::test]
async fn create_task_applies_defaults_and_normalizes_input() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let task = create_task(
        &server,
        &token,
        json!({ "title": "  Write report  ", "description": "   " }),
    )
    .await;

    assert_eq!(task["title"], "Write report"); // trimmed
    assert_eq!(task["description"], Value::Null); // blank becomes null
    assert_eq!(task["done"], false);
    assert_eq!(task["priority"], "medium");
    assert_eq!(task["due_date"], Value::Null);
    assert!(task.get("user_id").is_none());
}

#[tokio::test]
async fn task_endpoints_require_authentication() {
    let server = spawn_app().await;

    server
        .get("/tasks")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    server
        .post("/tasks")
        .json(&json!({ "title": "x" }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_task_validates_input() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let blank = server
        .post("/tasks")
        .authorization_bearer(&token)
        .json(&json!({ "title": "   " }))
        .await;
    blank.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert!(blank.json::<Value>()["details"]["title"].is_array());

    server
        .post("/tasks")
        .authorization_bearer(&token)
        .json(&json!({ "title": "x".repeat(201) }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Shape errors are 400: unknown priority, malformed date, missing title.
    for body in [
        json!({ "title": "x", "priority": "urgent" }),
        json!({ "title": "x", "due_date": "next tuesday" }),
        json!({ "description": "no title" }),
    ] {
        server
            .post("/tasks")
            .authorization_bearer(&token)
            .json(&body)
            .await
            .assert_status(StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn clients_cannot_set_server_controlled_fields() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let response = server
        .post("/tasks")
        .authorization_bearer(&token)
        .json(&json!({ "title": "x", "user_id": "0190b7a0-0000-7000-8000-000000000000" }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_update_and_delete_a_task() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    let task = create_task(
        &server,
        &token,
        json!({ "title": "Buy milk", "description": "2 litres", "due_date": "2026-12-01" }),
    )
    .await;
    let url = format!("/tasks/{}", task["id"].as_str().unwrap());

    let fetched: Value = server.get(&url).authorization_bearer(&token).await.json();
    assert_eq!(fetched, task);

    // PATCH changes only the fields it mentions; `null` clears a field.
    let updated = server
        .patch(&url)
        .authorization_bearer(&token)
        .json(&json!({ "done": true, "description": null }))
        .await;
    updated.assert_status_ok();
    let updated: Value = updated.json();
    assert_eq!(updated["done"], true);
    assert_eq!(updated["description"], Value::Null);
    assert_eq!(updated["title"], "Buy milk"); // untouched
    assert_eq!(updated["due_date"], "2026-12-01"); // untouched
    assert_ne!(updated["updated_at"], task["updated_at"]);
    assert_eq!(updated["created_at"], task["created_at"]);

    server
        .delete(&url)
        .authorization_bearer(&token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    server
        .get(&url)
        .authorization_bearer(&token)
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn patch_validates_like_create() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    let task = create_task(&server, &token, json!({ "title": "x" })).await;
    let url = format!("/tasks/{}", task["id"].as_str().unwrap());

    server
        .patch(&url)
        .authorization_bearer(&token)
        .json(&json!({ "title": "" }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    server
        .patch(&url)
        .authorization_bearer(&token)
        .json(&json!({ "description": "d".repeat(2001) }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn users_cannot_touch_each_others_tasks() {
    let server = spawn_app().await;
    let alice = register(&server, "alice@example.com").await;
    let mallory = register(&server, "mallory@example.com").await;
    let task = create_task(&server, &alice, json!({ "title": "Alice's secret" })).await;
    let url = format!("/tasks/{}", task["id"].as_str().unwrap());

    // 404, not 403: Mallory can't even learn that the task exists.
    server
        .get(&url)
        .authorization_bearer(&mallory)
        .await
        .assert_status(StatusCode::NOT_FOUND);
    server
        .patch(&url)
        .authorization_bearer(&mallory)
        .json(&json!({ "title": "pwned" }))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    server
        .delete(&url)
        .authorization_bearer(&mallory)
        .await
        .assert_status(StatusCode::NOT_FOUND);

    let list: Value = server
        .get("/tasks")
        .authorization_bearer(&mallory)
        .await
        .json();
    assert_eq!(list["total"], 0);

    // And Alice's task is unchanged.
    let still: Value = server.get(&url).authorization_bearer(&alice).await.json();
    assert_eq!(still["title"], "Alice's secret");
}

#[tokio::test]
async fn non_uuid_ids_are_a_bad_request() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let response = server.get("/tasks/42").authorization_bearer(&token).await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert!(response.json::<Value>()["error"].is_string());
}

#[tokio::test]
async fn unknown_routes_and_methods_get_json_errors() {
    let server = spawn_app().await;

    let not_found = server.get("/nope").await;
    not_found.assert_status(StatusCode::NOT_FOUND);
    assert_eq!(not_found.json::<Value>()["error"], "not found");

    let not_allowed = server.put("/tasks").await;
    not_allowed.assert_status(StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(not_allowed.json::<Value>()["error"], "method not allowed");

    server.get("/health").await.assert_status_ok();
}
