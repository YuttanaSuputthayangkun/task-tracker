//! Every query here includes `user_id = ?`. That one condition is the app's
//! authorization: a task belonging to someone else is indistinguishable from
//! a task that doesn't exist. Putting it in the SQL (rather than loading the
//! task and checking ownership in Rust afterwards) means there's no code path
//! that can forget the check.

use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::models::task::{SortField, SortOrder, Task, TaskFilter};

/// The column list, written once so every SELECT returns the same shape that
/// `Task`'s `FromRow` expects. Named columns rather than `SELECT *`, which
/// would break silently if a column were added or reordered.
const TASK_COLUMNS: &str =
    "id, user_id, title, description, done, priority, due_date, created_at, updated_at";

pub async fn insert(db: &SqlitePool, task: &Task) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tasks
            (id, user_id, title, description, done, priority, due_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(task.id)
    .bind(task.user_id)
    .bind(&task.title)
    .bind(&task.description)
    .bind(task.done)
    .bind(task.priority)
    .bind(task.due_date)
    .bind(task.created_at)
    .bind(task.updated_at)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_for_user(
    db: &SqlitePool,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<Task>, sqlx::Error> {
    sqlx::query_as::<_, Task>(&format!(
        // format! here only splices in our own constant column list, never
        // user input. Values still go through bind().
        "SELECT {TASK_COLUMNS} FROM tasks WHERE id = ? AND user_id = ?"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Writes every editable column of an existing task. Returns `false` if no row
/// matched, i.e. the task doesn't exist or isn't this user's.
pub async fn update(db: &SqlitePool, task: &Task) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE tasks
         SET title = ?, description = ?, done = ?, priority = ?, due_date = ?, updated_at = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(&task.title)
    .bind(&task.description)
    .bind(task.done)
    .bind(task.priority)
    .bind(task.due_date)
    .bind(task.updated_at)
    .bind(task.id)
    .bind(task.user_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Returns `false` if nothing was deleted.
pub async fn delete_for_user(
    db: &SqlitePool,
    id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// One page of a user's tasks, filtered and sorted.
///
/// The WHERE clause depends on which filters the client sent, so the SQL
/// can't be a single fixed string. `QueryBuilder` assembles it piece by piece
/// while keeping every *value* a bind parameter (`push_bind`). Only fixed
/// fragments of SQL that we wrote ourselves are ever `push`ed as text.
pub async fn list_for_user(
    db: &SqlitePool,
    user_id: Uuid,
    filter: &TaskFilter,
    sort: SortField,
    order: SortOrder,
    limit: i64,
    offset: i64,
) -> Result<Vec<Task>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(format!("SELECT {TASK_COLUMNS} FROM tasks"));
    push_where(&mut query, user_id, filter);
    query.push(" ORDER BY ").push(order_by(sort, order));
    query.push(" LIMIT ").push_bind(limit);
    query.push(" OFFSET ").push_bind(offset);

    query.build_query_as::<Task>().fetch_all(db).await
}

/// How many tasks match the filter in total, across all pages. The client
/// needs this to know how many pages there are.
pub async fn count_for_user(
    db: &SqlitePool,
    user_id: Uuid,
    filter: &TaskFilter,
) -> Result<i64, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM tasks");
    // The *same* function builds the WHERE clause for both queries, so the
    // count and the page can never disagree about which tasks match.
    push_where(&mut query, user_id, filter);

    query.build_query_scalar::<i64>().fetch_one(db).await
}

fn push_where(query: &mut QueryBuilder<'_, Sqlite>, user_id: Uuid, filter: &TaskFilter) {
    // Always first, never optional.
    query.push(" WHERE user_id = ").push_bind(user_id);

    if let Some(done) = filter.done {
        query.push(" AND done = ").push_bind(done);
    }
    if let Some(priority) = filter.priority {
        query.push(" AND priority = ").push_bind(priority);
    }
    // Dates are stored as 'YYYY-MM-DD' text, which compares correctly as a
    // string. A task with no due date (NULL) never matches a comparison, so
    // these filters naturally exclude undated tasks.
    if let Some(due_before) = filter.due_before {
        query.push(" AND due_date <= ").push_bind(due_before);
    }
    if let Some(due_after) = filter.due_after {
        query.push(" AND due_date >= ").push_bind(due_after);
    }
    if let Some(search) = &filter.search {
        // In LIKE patterns, `%` and `_` are wildcards. Escape them so that
        // searching for "100%" finds the literal text "100%".
        let escaped = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        // SQLite's LIKE is case-insensitive for ASCII letters.
        query
            .push(" AND title LIKE ")
            .push_bind(format!("%{escaped}%"))
            .push(" ESCAPE '\\'");
    }
}

/// Builds the ORDER BY clause from the enums. Every string returned here is a
/// hard-coded literal, so no client input can reach this part of the SQL.
fn order_by(sort: SortField, order: SortOrder) -> String {
    let dir = match order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };
    let primary = match sort {
        SortField::CreatedAt => format!("created_at {dir}"),
        SortField::UpdatedAt => format!("updated_at {dir}"),
        // COLLATE NOCASE so "apple" and "Banana" sort alphabetically, not
        // uppercase-before-lowercase.
        SortField::Title => format!("title COLLATE NOCASE {dir}"),
        // Undated tasks go last in both directions: `due_date IS NULL` is 0
        // for dated tasks and 1 for undated ones, sorted ascending.
        SortField::DueDate => format!("due_date IS NULL, due_date {dir}"),
        // Sorting the text alphabetically would give high < low < medium, so
        // map each priority to its rank first.
        SortField::Priority => {
            format!("CASE priority WHEN 'low' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END {dir}")
        }
    };
    // Tie-breaker: without it, rows with equal sort values (two tasks with the
    // same priority) come back in an unspecified order that can differ between
    // queries, and pagination would skip or repeat them across pages. The id
    // is unique and, being a UUID v7, also roughly creation-ordered.
    format!("{primary}, id {dir}")
}
