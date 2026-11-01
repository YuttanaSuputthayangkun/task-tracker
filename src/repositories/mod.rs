//! The repository layer: the only code in the app that contains SQL.
//!
//! Why a separate layer:
//! * Every query lives in one place, so a reviewer can check "is every task
//!   query filtered by user_id?" by reading two files.
//! * Repositories return `sqlx::Error`, not HTTP errors. They don't know
//!   they're behind a web server; deciding what a failure *means* is the
//!   service's job.
//! * Switching databases (see GUIDE.md, "Moving to PostgreSQL") mostly means
//!   changing this layer and the migrations, and nothing above them.
//!
//! Functions take `&SqlitePool` rather than `&AppState`: a repository needs a
//! database and nothing else, and saying so in the signature keeps it that way.

pub mod tasks;
pub mod users;
