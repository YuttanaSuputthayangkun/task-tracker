//! Shared application state, handed to every handler through axum's `State`.
//!
//! Why not globals: state passed in explicitly can be built differently for
//! each test (every integration test gets its own fresh database), and a
//! handler's signature tells you what it depends on.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;

/// Created once at startup and cloned into every request. The clone is cheap:
/// `SqlitePool` is internally reference-counted and the config sits behind an
/// `Arc`, so cloning copies two pointers, not the data.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
}
