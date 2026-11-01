//! Data types, with no behaviour beyond conversions and validation rules.
//!
//! Each resource has up to three kinds of types, kept deliberately separate:
//!
//! | Kind         | Example        | Who defines what's in it                   |
//! |--------------|----------------|--------------------------------------------|
//! | Request DTO  | `CreateTask`   | the fields a client is *allowed to set*    |
//! | Domain model | `Task`         | what the database stores                   |
//! | Response DTO | `TaskResponse` | the fields a client is *allowed to see*    |
//!
//! That looks like duplication, but it is what prevents the two classic bugs:
//! leaking a field (returning `password_hash` because `User` derived
//! `Serialize`) and mass assignment (a client setting `user_id` because the
//! request body was deserialized straight into `Task`).

pub mod task;
pub mod user;

use serde::{Deserialize, Deserializer};
use validator::ValidationError;

/// A generic page of results, the envelope every list endpoint returns.
/// `total` lets a client compute the number of pages; `limit` and `offset`
/// echo back the values actually used (after clamping), not the ones asked for.
#[derive(Debug, serde::Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Validator for "must contain something other than whitespace". `length(min = 1)`
/// alone would accept `"   "` as a title.
pub fn not_blank(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        let mut error = ValidationError::new("blank");
        error.message = Some("must not be blank".into());
        return Err(error);
    }
    Ok(())
}

/// Serde helper for PATCH bodies, which need to tell three cases apart:
///
/// | JSON                      | Meaning          | Rust value          |
/// |---------------------------|------------------|---------------------|
/// | field absent              | leave unchanged  | `None`              |
/// | `"due_date": null`        | clear the value  | `Some(None)`        |
/// | `"due_date": "2026-12-01"`| set the value    | `Some(Some(date))`  |
///
/// Plain `Option<T>` can't do this: serde maps both "absent" and `null` to
/// `None`. Use together with `#[serde(default)]`, which supplies the `None`
/// when the field is absent; this function only runs when it's present.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
