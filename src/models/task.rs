use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use super::{double_option, not_blank};

/// One enum, three representations, all spelled the same way:
/// `"low"` in JSON (serde), `'low'` in the database (sqlx), `Priority::Low` in
/// Rust. Using an enum instead of a String means an invalid priority can't
/// exist anywhere in the program: serde rejects it on the way in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Medium,
    High,
}

/// A row of the `tasks` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Task {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub done: bool,
    pub priority: Priority,
    pub due_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body of `POST /tasks`. Note what's missing: `id`, `user_id`, `done` and the
/// timestamps. The server decides those, so a client simply can't send them
/// (`deny_unknown_fields` turns an attempt into a 400).
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateTask {
    #[validate(length(min = 1, max = 200), custom(function = "not_blank"))]
    pub title: String,
    #[serde(default)]
    #[validate(length(max = 2000))]
    pub description: Option<String>,
    #[serde(default)] // Missing means Priority::default(), i.e. Medium
    pub priority: Priority,
    #[serde(default)]
    pub due_date: Option<NaiveDate>,
}

/// Body of `PATCH /tasks/{id}`. Every field is optional: only the fields
/// present in the JSON are changed. See `double_option` for how the nullable
/// fields distinguish "leave alone" from "clear".
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateTask {
    #[validate(length(min = 1, max = 200), custom(function = "not_blank"))]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[validate(length(max = 2000))]
    pub description: Option<Option<String>>,
    pub done: Option<bool>,
    pub priority: Option<Priority>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date: Option<Option<NaiveDate>>,
}

/// What clients see of a task. `user_id` is left out: it's always the caller's
/// own id, so it adds nothing and only exposes internal identifiers.
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub done: bool,
    pub priority: Priority,
    pub due_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        Self {
            id: task.id,
            title: task.title,
            description: task.description,
            done: task.done,
            priority: task.priority,
            due_date: task.due_date,
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Listing: filtering, sorting and pagination
// ---------------------------------------------------------------------------

/// The query string of `GET /tasks`, e.g.
/// `/tasks?done=false&priority=high&q=report&sort=due_date&order=asc&limit=10&offset=20`
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)] // `?prority=high` is a 400, not a silently ignored filter
pub struct ListTasksQuery {
    pub done: Option<bool>,
    pub priority: Option<Priority>,
    /// Only tasks due on or before this date.
    pub due_before: Option<NaiveDate>,
    /// Only tasks due on or after this date.
    pub due_after: Option<NaiveDate>,
    /// Case-insensitive substring search in the title.
    #[validate(length(max = 200))]
    pub q: Option<String>,
    #[serde(default)]
    pub sort: SortField,
    #[serde(default)]
    pub order: SortOrder,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// The columns a client may sort by. An enum rather than a free-form string
/// because sort columns can't be passed as SQL bind parameters: they have to
/// be written into the SQL text itself. Matching on a closed enum guarantees
/// that only these exact, hard-coded column names can ever reach the query.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    CreatedAt,
    UpdatedAt,
    DueDate,
    Priority,
    Title,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

/// The filters, after the service has cleaned up the raw query. Kept separate
/// from `ListTasksQuery` so the repository doesn't depend on how the HTTP
/// query string happens to be spelled.
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub done: Option<bool>,
    pub priority: Option<Priority>,
    pub due_before: Option<NaiveDate>,
    pub due_after: Option<NaiveDate>,
    pub search: Option<String>,
}
