use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use uuid::Uuid;
use validator::Validate;

use crate::{
    error::AppError,
    extractors::{AppJson, AppPath, AppQuery, AuthUser},
    models::{
        Page,
        task::{CreateTask, ListTasksQuery, TaskResponse, UpdateTask},
    },
    services::tasks as task_service,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        // axum 0.8 writes path parameters as {id}. Older tutorials use :id.
        .route(
            "/tasks/{id}",
            get(get_task).patch(update_task).delete(delete_task),
        )
}

// Every handler below takes `user: AuthUser`, so every task endpoint requires
// a valid token. `user.id` is then passed to the service, which scopes every
// query to that user.

/// `GET /tasks?done=&priority=&due_before=&due_after=&q=&sort=&order=&limit=&offset=`
async fn list_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    AppQuery(query): AppQuery<ListTasksQuery>,
) -> Result<Json<Page<TaskResponse>>, AppError> {
    query.validate()?;
    let page = task_service::list(&state.db, user.id, query).await?;
    Ok(Json(Page {
        items: page.items.into_iter().map(TaskResponse::from).collect(),
        total: page.total,
        limit: page.limit,
        offset: page.offset,
    }))
}

/// `POST /tasks`: 201 Created with the new task.
async fn create_task(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(payload): AppJson<CreateTask>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    payload.validate()?;
    let task = task_service::create(&state.db, user.id, payload).await?;
    Ok((StatusCode::CREATED, Json(task.into())))
}

/// `GET /tasks/{id}`: 404 if it doesn't exist *or* isn't yours.
async fn get_task(
    State(state): State<AppState>,
    user: AuthUser,
    // A non-UUID id is rejected with 400 before this handler runs.
    AppPath(id): AppPath<Uuid>,
) -> Result<Json<TaskResponse>, AppError> {
    let task = task_service::get(&state.db, user.id, id).await?;
    Ok(Json(task.into()))
}

/// `PATCH /tasks/{id}`: partial update; only the fields sent are changed.
async fn update_task(
    State(state): State<AppState>,
    user: AuthUser,
    AppPath(id): AppPath<Uuid>,
    AppJson(patch): AppJson<UpdateTask>,
) -> Result<Json<TaskResponse>, AppError> {
    patch.validate()?;
    let task = task_service::update(&state.db, user.id, id, patch).await?;
    Ok(Json(task.into()))
}

/// `DELETE /tasks/{id}`: 204 No Content on success.
async fn delete_task(
    State(state): State<AppState>,
    user: AuthUser,
    AppPath(id): AppPath<Uuid>,
) -> Result<StatusCode, AppError> {
    task_service::delete(&state.db, user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
