//! Filtering, sorting and pagination of `GET /tasks`.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::{create_task, register, spawn_app};
use serde_json::{Value, json};

/// Fetches `/tasks` with the given query string and returns the body.
async fn list(server: &TestServer, token: &str, query: &str) -> Value {
    let response = server
        .get(&format!("/tasks{query}"))
        .authorization_bearer(token)
        .await;
    response.assert_status_ok();
    response.json()
}

/// The titles of the items in a list response, in order.
fn titles(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect()
}

/// A small fixed data set used by most tests below.
async fn seed(server: &TestServer, token: &str) {
    for body in [
        json!({ "title": "Apple", "priority": "low", "due_date": "2026-11-10" }),
        json!({ "title": "banana", "priority": "high", "due_date": "2026-11-05" }),
        json!({ "title": "Cherry", "priority": "medium" }),
        json!({ "title": "Discount 100%", "priority": "high", "due_date": "2026-11-20" }),
        json!({ "title": "Discount 1000", "priority": "low" }),
    ] {
        create_task(server, token, body).await;
    }
}

#[tokio::test]
async fn default_listing_is_newest_first_in_an_envelope() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page = list(&server, &token, "").await;

    assert_eq!(page["total"], 5);
    assert_eq!(page["limit"], 20);
    assert_eq!(page["offset"], 0);
    assert_eq!(
        titles(&page),
        [
            "Discount 1000",
            "Discount 100%",
            "Cherry",
            "banana",
            "Apple"
        ]
    );
}

#[tokio::test]
async fn filters_by_done_and_priority() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;
    let all = list(&server, &token, "?sort=title&order=asc").await;
    let apple_id = all["items"][0]["id"].as_str().unwrap();
    server
        .patch(&format!("/tasks/{apple_id}"))
        .authorization_bearer(&token)
        .json(&json!({ "done": true }))
        .await
        .assert_status_ok();

    let done = list(&server, &token, "?done=true").await;
    assert_eq!(titles(&done), ["Apple"]);

    let open_low = list(&server, &token, "?done=false&priority=low").await;
    assert_eq!(titles(&open_low), ["Discount 1000"]);
    assert_eq!(open_low["total"], 1);
}

#[tokio::test]
async fn filters_by_due_date_range() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page = list(
        &server,
        &token,
        "?due_after=2026-11-06&due_before=2026-11-30&sort=due_date&order=asc",
    )
    .await;
    // Tasks without a due date never match a date filter.
    assert_eq!(titles(&page), ["Apple", "Discount 100%"]);
}

#[tokio::test]
async fn search_is_case_insensitive_and_treats_wildcards_literally() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page = list(&server, &token, "?q=BANANA").await;
    assert_eq!(titles(&page), ["banana"]);

    // `%` must match a literal percent sign, not "anything". (%25 is how `%`
    // is written in a URL.)
    let page = list(&server, &token, "?q=100%25").await;
    assert_eq!(titles(&page), ["Discount 100%"]);
}

#[tokio::test]
async fn sorts_by_priority_rank_not_alphabetically() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page = list(&server, &token, "?sort=priority&order=desc").await;
    let priorities: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["priority"].as_str().unwrap())
        .collect();
    assert_eq!(priorities, ["high", "high", "medium", "low", "low"]);
}

#[tokio::test]
async fn sorting_by_due_date_puts_undated_tasks_last_in_both_directions() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let asc = list(&server, &token, "?sort=due_date&order=asc").await;
    assert_eq!(&titles(&asc)[..3], ["banana", "Apple", "Discount 100%"]);

    let desc = list(&server, &token, "?sort=due_date&order=desc").await;
    assert_eq!(&titles(&desc)[..3], ["Discount 100%", "Apple", "banana"]);
}

#[tokio::test]
async fn title_sort_ignores_case() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page = list(&server, &token, "?sort=title&order=asc").await;
    assert_eq!(
        titles(&page),
        [
            "Apple",
            "banana",
            "Cherry",
            "Discount 100%",
            "Discount 1000"
        ]
    );
}

#[tokio::test]
async fn paginates_with_limit_and_offset() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;
    seed(&server, &token).await;

    let page1 = list(&server, &token, "?sort=title&order=asc&limit=2").await;
    let page2 = list(&server, &token, "?sort=title&order=asc&limit=2&offset=2").await;
    let page3 = list(&server, &token, "?sort=title&order=asc&limit=2&offset=4").await;

    assert_eq!(titles(&page1), ["Apple", "banana"]);
    assert_eq!(titles(&page2), ["Cherry", "Discount 100%"]);
    assert_eq!(titles(&page3), ["Discount 1000"]);
    // `total` counts all matches, not just this page.
    assert_eq!(page3["total"], 5);
    assert_eq!(page3["offset"], 4);
}

#[tokio::test]
async fn page_size_is_clamped() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    let huge = list(&server, &token, "?limit=100000").await;
    assert_eq!(huge["limit"], 100);

    let zero = list(&server, &token, "?limit=0&offset=-5").await;
    assert_eq!(zero["limit"], 1);
    assert_eq!(zero["offset"], 0);
}

#[tokio::test]
async fn bad_query_parameters_are_rejected() {
    let server = spawn_app().await;
    let token = register(&server, "alice@example.com").await;

    for query in [
        "?prority=high",       // typo: unknown parameter
        "?sort=password_hash", // not an allowed sort column
        "?order=sideways",
        "?done=maybe",
        "?due_before=tomorrow",
    ] {
        let response = server
            .get(&format!("/tasks{query}"))
            .authorization_bearer(&token)
            .await;
        response.assert_status(StatusCode::BAD_REQUEST);
        assert!(response.json::<Value>()["error"].is_string(), "{query}");
    }
}
