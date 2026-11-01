//! Task use cases. Every function takes the caller's `user_id` and passes it
//! down to the repository, which uses it in every query.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        Page,
        task::{CreateTask, ListTasksQuery, Task, TaskFilter, UpdateTask},
    },
    repositories::tasks as task_repo,
};

/// Never trust the client's page size: `?limit=1000000` would load the whole
/// table into memory in one request.
const MAX_PAGE_SIZE: i64 = 100;

#[tracing::instrument(skip(db, payload))]
pub async fn create(db: &SqlitePool, user_id: Uuid, payload: CreateTask) -> Result<Task, AppError> {
    let now = Utc::now();
    let task = Task {
        // UUID v7 starts with a timestamp, so new ids are roughly increasing.
        // New rows then land at the end of the primary-key index instead of
        // at random positions in it, as random v4 ids would.
        id: Uuid::now_v7(),
        user_id,
        title: payload.title.trim().to_string(),
        description: normalize_description(payload.description),
        done: false,
        priority: payload.priority,
        due_date: payload.due_date,
        created_at: now,
        updated_at: now,
    };
    task_repo::insert(db, &task).await?;
    tracing::info!(task_id = %task.id, "task created");
    Ok(task)
}

pub async fn get(db: &SqlitePool, user_id: Uuid, id: Uuid) -> Result<Task, AppError> {
    task_repo::find_for_user(db, id, user_id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn list(
    db: &SqlitePool,
    user_id: Uuid,
    query: ListTasksQuery,
) -> Result<Page<Task>, AppError> {
    // Clamp rather than reject. A client asking for 500 gets 100, and the
    // response's `limit` field tells it so.
    let limit = query.limit.clamp(1, MAX_PAGE_SIZE);
    let offset = query.offset.max(0);

    let filter = TaskFilter {
        done: query.done,
        priority: query.priority,
        due_before: query.due_before,
        due_after: query.due_after,
        // `?q=` (empty) means "no search", not "titles containing nothing".
        search: query
            .q
            .map(|q| q.trim().to_string())
            .filter(|q| !q.is_empty()),
    };

    // Two queries: one for the page, one for the total. They run one after
    // the other, so a task created in between could make `total` off by one
    // for that single response. That's normal for offset pagination and
    // harmless here.
    let items =
        task_repo::list_for_user(db, user_id, &filter, query.sort, query.order, limit, offset)
            .await?;
    let total = task_repo::count_for_user(db, user_id, &filter).await?;

    Ok(Page {
        items,
        total,
        limit,
        offset,
    })
}

#[tracing::instrument(skip(db, patch))]
pub async fn update(
    db: &SqlitePool,
    user_id: Uuid,
    id: Uuid,
    patch: UpdateTask,
) -> Result<Task, AppError> {
    // Load, change in Rust, save. That's simpler and clearer than one clever
    // UPDATE statement with COALESCE for every field, especially for fields
    // that can be explicitly cleared to NULL. The cost: if two requests edit
    // the same task at the same moment, the later save wins. That's
    // acceptable for a personal task list.
    let mut task = get(db, user_id, id).await?;

    if let Some(title) = patch.title {
        task.title = title.trim().to_string();
    }
    if let Some(description) = patch.description {
        task.description = normalize_description(description);
    }
    if let Some(done) = patch.done {
        task.done = done;
    }
    if let Some(priority) = patch.priority {
        task.priority = priority;
    }
    if let Some(due_date) = patch.due_date {
        task.due_date = due_date;
    }
    task.updated_at = Utc::now();

    // The task could have been deleted between our read and this write.
    if !task_repo::update(db, &task).await? {
        return Err(AppError::NotFound);
    }
    tracing::info!(task_id = %task.id, "task updated");
    Ok(task)
}

#[tracing::instrument(skip(db))]
pub async fn delete(db: &SqlitePool, user_id: Uuid, id: Uuid) -> Result<(), AppError> {
    if !task_repo::delete_for_user(db, id, user_id).await? {
        return Err(AppError::NotFound);
    }
    tracing::info!(task_id = %id, "task deleted");
    Ok(())
}

/// A business rule: a description that is empty or only whitespace is stored
/// as "no description" (NULL), so clients don't have to handle both.
fn normalize_description(description: Option<String>) -> Option<String> {
    description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
}
