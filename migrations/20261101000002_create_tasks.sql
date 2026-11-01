-- Tasks, each owned by exactly one user.
--
-- * ON DELETE CASCADE: if a user is deleted, their tasks go with them, so we
--   can never end up with orphaned rows that point to a missing user.
--   (SQLite only enforces foreign keys when the `foreign_keys` pragma is on;
--   src/db.rs turns it on for every connection.)
-- * priority is TEXT guarded by a CHECK constraint, so the database rejects
--   anything other than the three known values, even if a bug in the app
--   (or someone using the sqlite3 shell) tries to write one.
-- * due_date is a calendar date ('YYYY-MM-DD'), not a timestamp: "due on
--   Friday" means Friday wherever you are, so it has no time zone.
CREATE TABLE tasks (
    id          BLOB PRIMARY KEY NOT NULL,
    user_id     BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    description TEXT,
    done        BOOLEAN NOT NULL DEFAULT FALSE,
    priority    TEXT NOT NULL DEFAULT 'medium'
                CHECK (priority IN ('low', 'medium', 'high')),
    due_date    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Every task query filters by user_id first (a user only ever sees their own
-- tasks), so every index starts with user_id. The second column serves the
-- most common sort orders: "newest first" and "due soonest".
CREATE INDEX idx_tasks_user_created ON tasks(user_id, created_at);
CREATE INDEX idx_tasks_user_due     ON tasks(user_id, due_date);
