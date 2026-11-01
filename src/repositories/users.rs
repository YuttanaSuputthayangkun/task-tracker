use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::user::User;

pub async fn insert(db: &SqlitePool, user: &User) -> Result<(), sqlx::Error> {
    // The `?` placeholders are bind parameters. The values travel to the
    // database separately from the SQL text, so they can never be interpreted
    // as SQL, whatever characters they contain.
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(user.id)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(user.created_at)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_by_email(db: &SqlitePool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at
         FROM users
         WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(db) // No row gives Ok(None), not an error
    .await
}

pub async fn find_by_id(db: &SqlitePool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at
         FROM users
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await
}
