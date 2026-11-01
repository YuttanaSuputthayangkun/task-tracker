-- Users who can sign up and log in.
--
-- Why these column types:
-- * id is a UUID stored as 16 raw bytes (BLOB). That's how sqlx encodes `Uuid`
--   for SQLite, and it's half the size of the 36-character text form.
-- * Timestamps are TEXT in RFC 3339 format. SQLite has no real date/time type;
--   sqlx encodes chrono's DateTime<Utc> as text, and ISO-8601 text sorts
--   correctly as plain strings.
-- * email is UNIQUE. The database, not the application, is the final
--   authority on uniqueness: two simultaneous signups with the same email
--   would both pass an "is this email taken?" check in Rust, but only one
--   INSERT can win against a UNIQUE constraint. UNIQUE also creates an index,
--   which the login lookup (WHERE email = ?) needs anyway.
CREATE TABLE users (
    id            BLOB PRIMARY KEY NOT NULL,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
