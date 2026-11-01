//! The service layer: business rules, with no HTTP and no SQL.
//!
//! Why a separate layer between routes and repositories:
//! * Handlers stay thin. They only translate HTTP into a function call and
//!   back, so the actual rules ("normalize the email", "a PATCH only changes
//!   the fields it mentions") aren't buried among extractors and status codes.
//! * Nothing here imports axum. The same functions could be called from a CLI
//!   tool, a background job, or a different API.
//! * Services decide what a low-level failure *means*: a UNIQUE-constraint
//!   error from the database becomes "email already registered" (409), an
//!   update that matched no rows becomes "not found" (404).

pub mod auth;
pub mod tasks;
