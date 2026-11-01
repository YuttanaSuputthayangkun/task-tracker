# Task Tracker: how it fits together

This guide walks through the Task Tracker API: how to run it, how the code is organized and why, and what happens to a single request on its way through the app. [DEPENDENCIES.md](DEPENDENCIES.md) explains the individual crates. This guide explains how they're assembled.

**Contents**

1. [Running it](#1-running-it)
2. [The API at a glance](#2-the-api-at-a-glance)
3. [Module structure](#3-module-structure)
4. [How a request flows through the app](#4-how-a-request-flows-through-the-app)
5. [Authentication: signup, login, tokens](#5-authentication-signup-login-tokens)
6. [Authorization: "only your own tasks"](#6-authorization-only-your-own-tasks)
7. [Errors: one shape for everything](#7-errors-one-shape-for-everything)
8. [The database schema](#8-the-database-schema)
9. [Filtering, sorting and pagination](#9-filtering-sorting-and-pagination)
10. [Tests](#10-tests)
11. [Moving to PostgreSQL later](#11-moving-to-postgresql-later)
12. [Where DEPENDENCIES.md's examples had to change](#12-where-dependenciesmds-examples-had-to-change)
13. [Ideas for what to build next](#13-ideas-for-what-to-build-next)

---

## 1. Running it

**One-time setup**

- Rust via [rustup](https://rustup.rs). On Windows this also needs the Visual Studio C++ Build Tools, which the rustup installer offers to install.
- In VS Code, install the **rust-analyzer** extension (code completion, inline types, go-to-definition). **Even Better TOML** helps with `Cargo.toml`. The **Markdown Preview Mermaid Support** extension renders the diagrams in this file inside VS Code's Markdown preview (`Ctrl+Shift+V`).
- Configuration lives in `.env`, which is git-ignored. `.env.example` lists every setting. A `.env` with a randomly generated `JWT_SECRET` has already been created for you. If you clone the repository elsewhere, copy `.env.example` to `.env` and set a new secret.

**Run the server**

```powershell
cargo run
# ... INFO task_tracker: listening on http://127.0.0.1:3000
```

On first start the server creates `tasks.db` next to `Cargo.toml` and runs the migrations. Stop it with `Ctrl+C`; it finishes any in-flight requests first ("graceful shutdown").

> **Use `127.0.0.1`, not `localhost`, on Windows.** `localhost` resolves to the IPv6 address `::1` first. The server only listens on IPv4, so every request waits about 2 seconds for the fallback. With `127.0.0.1` the same request takes about 20 ms.

**Run the tests**

```powershell
cargo test                       # everything
cargo test --test task_list      # one test file
cargo test login                 # tests whose name contains "login"
```

**Try it by hand (PowerShell)**

PowerShell's `Invoke-RestMethod` parses JSON responses into objects, so it's more convenient than `curl` here:

```powershell
$base = "http://127.0.0.1:3000"
$creds = '{"email":"me@example.com","password":"supersecret123"}'

Invoke-RestMethod -Method Post "$base/auth/signup" -ContentType application/json -Body $creds
$token = (Invoke-RestMethod -Method Post "$base/auth/login" -ContentType application/json -Body $creds).access_token
$auth = @{ Authorization = "Bearer $token" }

Invoke-RestMethod -Method Post "$base/tasks" -Headers $auth -ContentType application/json `
  -Body '{"title":"Learn Axum","priority":"high","due_date":"2026-11-15"}'

Invoke-RestMethod "$base/tasks?priority=high&sort=due_date&order=asc" -Headers $auth |
  ConvertTo-Json -Depth 5
```

When a request fails, `Invoke-RestMethod` throws. The JSON error body is then in `$_.ErrorDetails.Message` inside a `try { } catch { }` block.

To look inside the database, use the **SQLite Viewer** extension in VS Code, or the `sqlite3` command-line tool. Stop the server first if you plan to write to the file.

---

## 2. The API at a glance

| Method | Path | Auth | Success | Purpose |
|---|---|---|---|---|
| `GET` | `/health` | no | 200 | Liveness check |
| `POST` | `/auth/signup` | no | 201 | Create an account `{email, password}` |
| `POST` | `/auth/login` | no | 200 | Get a token `{access_token, token_type, expires_in}` |
| `GET` | `/auth/me` | yes | 200 | The current user |
| `GET` | `/tasks` | yes | 200 | List your tasks (filter/sort/paginate, see §9) |
| `POST` | `/tasks` | yes | 201 | Create `{title, description?, priority?, due_date?}` |
| `GET` | `/tasks/{id}` | yes | 200 | One task |
| `PATCH` | `/tasks/{id}` | yes | 200 | Change some fields; `null` clears `description` / `due_date` |
| `DELETE` | `/tasks/{id}` | yes | 204 | Delete |

"Auth: yes" means the request needs an `Authorization: Bearer <access_token>` header.

A task looks like this:

```json
{
  "id": "01a1978f-1466-7416-af5c-a3f83dcb89b9",
  "title": "Learn Axum",
  "description": null,
  "done": false,
  "priority": "high",
  "due_date": "2026-11-15",
  "created_at": "2026-11-01T14:41:38.918192300Z",
  "updated_at": "2026-11-01T14:41:38.918192300Z"
}
```

---

## 3. Module structure

```
task-tracker/
├── Cargo.toml          dependencies
├── build.rs            rebuild when migrations change
├── .env.example        documents every setting
├── migrations/         versioned SQL, applied in filename order
├── src/
│   ├── main.rs         startup only: config → DB → migrate → serve
│   ├── lib.rs          build_app(): router + middleware
│   ├── config.rs       typed settings from the environment
│   ├── db.rs           connection pool + migrations
│   ├── state.rs        AppState shared by all handlers
│   ├── error.rs        AppError → HTTP status + JSON body
│   ├── extractors.rs   AuthUser, AppJson / AppQuery / AppPath
│   ├── routes/         HTTP: URLs, handlers, status codes
│   ├── services/       business rules, passwords, tokens
│   ├── repositories/   SQL
│   └── models/         request / domain / response types
└── tests/              integration tests (the real router, in-memory DB)
```

### Who calls whom

```mermaid
flowchart TD
    main["main.rs<br/>startup"] --> lib["lib.rs<br/>build_app()"]
    main --> config["config.rs"]
    main --> db["db.rs"]
    lib --> routes["routes/<br/>auth.rs · tasks.rs"]

    routes --> extractors["extractors.rs<br/>AuthUser · AppJson · AppPath · AppQuery"]
    routes --> services["services/<br/>auth.rs · tasks.rs"]
    extractors --> services
    services --> repositories["repositories/<br/>users.rs · tasks.rs"]
    repositories --> sqlite[("SQLite")]

    models["models/<br/>user.rs · task.rs"]
    error["error.rs<br/>AppError"]
    state["state.rs<br/>AppState"]

    routes -.-> models
    services -.-> models
    repositories -.-> models
    routes -.-> error
    services -.-> error
    extractors -.-> error
    routes -.-> state
    extractors -.-> state

    classDef shared fill:#eef,stroke:#88a
    class models,error,state shared
```

Solid arrows are calls, and they only point **downwards**. Dotted arrows point to shared definitions that several layers use.

### Why layers?

Each layer answers exactly one question, and knows as little as possible about the others:

| Layer | Question it answers | Imports axum? | Contains SQL? |
|---|---|---|---|
| **routes** | Which URL runs what, with which status code? | yes | no |
| **extractors** | Is the request well-formed? Who is the caller? | yes | no |
| **services** | What are the rules? ("trim titles", "PATCH changes only what's sent") | no | no |
| **repositories** | How is it stored and fetched? | no | yes |
| **models** | What does the data look like? | no | no |

What that buys you in practice:

- **You know where to look.** A wrong status code is in a route. A wrong rule is in a service. A wrong result set is in a repository.
- **Security checks are reviewable.** "Is every task query scoped to the user?" means reading one file, `repositories/tasks.rs`.
- **Changes stay local.** Switching to PostgreSQL mostly touches repositories and migrations (§11). Adding a CLI or a background job reuses the services unchanged.
- **Each part can be tested at the right level.** Services can be called directly, without HTTP.

The cost is more files and some "pass-through" code, such as a handler that just calls a service. For an app this size that cost is small, and it's how most production codebases are organized.

### Why three kinds of types per resource

`models/` holds, for tasks, `CreateTask` / `UpdateTask` (what clients may **send**), `Task` (what the DB **stores**) and `TaskResponse` (what clients may **see**). `Task` has a `user_id`. `CreateTask` doesn't, so a client can't create tasks for someone else. `TaskResponse` doesn't either, so internal ids aren't exposed. `User` has a `password_hash`, and it doesn't derive `Serialize`, so it *can't* be sent to a client. The compiler would refuse.

---

## 4. How a request flows through the app

Here is `POST /tasks` with a valid token and body, end to end:

```mermaid
sequenceDiagram
    autonumber
    participant C as Client
    participant MW as Middleware<br/>(Trace, Timeout, CORS)
    participant R as Router
    participant X as Extractors
    participant H as Handler<br/>routes/tasks.rs
    participant S as Service<br/>services/tasks.rs
    participant Repo as Repository<br/>repositories/tasks.rs
    participant DB as SQLite

    C->>MW: POST /tasks<br/>Authorization: Bearer eyJ...<br/>{"title":"Learn Axum"}
    MW->>R: (starts log span, starts 10s timer)
    R->>X: route matched: create_task
    X->>X: State → AppState (pool + config)
    X->>X: AuthUser → verify JWT signature + expiry
    X->>X: AppJson<CreateTask> → parse body
    X->>H: create_task(state, user, payload)
    H->>H: payload.validate()
    H->>S: task_service::create(db, user.id, payload)
    S->>S: new UUID v7, trim title, timestamps
    S->>Repo: task_repo::insert(db, &task)
    Repo->>DB: INSERT INTO tasks (...) VALUES (?, ?, ...)
    DB-->>Repo: ok
    Repo-->>S: Ok(())
    S-->>H: Ok(Task)
    H-->>MW: 201 Created + Json(TaskResponse)
    MW-->>C: response (+ log line with status & latency)
```

Step by step, with where to find each part:

1. **tokio** accepts the TCP connection, and **hyper** (inside axum) parses the HTTP. You don't see this code.
2. **Middleware** (`lib.rs`, `build_app`) wraps every request. `TraceLayer` logs it, `TimeoutLayer` cancels it after 10 s, and `CorsLayer` handles browser cross-origin rules.
3. **The router** (`routes/mod.rs` and `routes/tasks.rs`) matches `POST /tasks` to `create_task`.
4. **Extractors** run in argument order (`extractors.rs`). Each one either produces a value or **rejects** the request. A rejection becomes the response, and the handler never runs. This is where authentication and parsing happen.
5. **The handler** validates the DTO, calls the service, and chooses the status code.
6. **The service** applies the business rules and builds the domain `Task`.
7. **The repository** runs parameterized SQL.
8. **On the way back**, the `Result` from each layer flows up through `?`. The handler's return value becomes the HTTP response via `IntoResponse`.

### The same request, when things go wrong

Every failure exits through the same door: an `AppError`, converted to a response in `error.rs`.

```mermaid
flowchart TD
    A[Request arrives] --> B{Route exists?}
    B -- no --> E404["404 not found<br/>(router fallback)"]
    B -- "yes, wrong method" --> E405["405 method not allowed"]
    B -- yes --> C{"AuthUser:<br/>valid Bearer token?"}
    C -- no --> E401["401 invalid or missing credentials"]
    C -- yes --> D{"AppJson / AppPath / AppQuery:<br/>parses?"}
    D -- no --> E400["400 + parser message"]
    D -- yes --> V{"payload.validate()"}
    V -- fails --> E422["422 + details per field"]
    V -- ok --> S{"Service / repository"}
    S -- "row not found /<br/>not yours" --> E404b["404 not found"]
    S -- "UNIQUE violation<br/>(signup)" --> E409["409 conflict"]
    S -- "DB down, bug, ..." --> E500["500 internal server error<br/>(details logged, not sent)"]
    S -- ok --> OK["2xx + JSON"]
```

---

## 5. Authentication: signup, login, tokens

```mermaid
sequenceDiagram
    participant C as Client
    participant API as API
    participant DB as SQLite

    Note over C,DB: Signup
    C->>API: POST /auth/signup {email, password}
    API->>API: validate (email format, 8–128 chars)
    API->>API: argon2 hash (on blocking thread pool)
    API->>DB: INSERT user (email lowercased, hash)
    API-->>C: 201 {id, email, created_at}

    Note over C,DB: Login
    C->>API: POST /auth/login {email, password}
    API->>DB: SELECT user WHERE email = ?
    API->>API: argon2 verify (against dummy hash if no user)
    API->>API: sign JWT {sub: user_id, iat, exp}
    API-->>C: 200 {access_token, token_type: "Bearer", expires_in}

    Note over C,DB: Every later request
    C->>API: GET /tasks  Authorization: Bearer <token>
    API->>API: AuthUser: check signature + exp, no DB lookup
    API->>DB: SELECT ... WHERE user_id = <sub from token>
```

Details worth noticing in `services/auth.rs`:

- **Passwords are never stored**, only argon2 hashes. Hashing runs in `spawn_blocking` because it deliberately takes tens of milliseconds of CPU. On tokio's async threads it would stall every other request scheduled on that thread.
- **Login never reveals whether an email exists.** An unknown email and a wrong password return the same 401 with the same body, *and* take the same time: an unknown email is checked against a dummy hash instead of returning early.
- **Signup relies on the database for uniqueness.** It inserts, and turns a UNIQUE-constraint violation into a 409. Checking first and then inserting would let two simultaneous signups both pass the check.
- **Tokens expire** (`JWT_TTL_MINUTES`, default 60). There's no logout endpoint, because a JWT can't be revoked before it expires. That's the trade-off you accepted by choosing JWT over server-side sessions. A client "logs out" by discarding the token.
- **The algorithm is pinned** to HS256 when verifying, so a forged token can't pick a weaker algorithm or `none`.

---

## 6. Authorization: "only your own tasks"

Authentication answers *who are you*. Authorization answers *what may you touch*. They live in different places on purpose:

- **Who:** the `AuthUser` extractor reads the user id from the verified token. A client can't choose it, because changing the id would break the signature.
- **What:** every query in `repositories/tasks.rs` includes `AND user_id = ?`, bound to that id.

The ownership check is part of the SQL, not an `if` in Rust after loading the row, so no code path can forget it. Another user's task simply doesn't match the query, and the API answers **404, not 403**. A 403 would confirm that a task with that id exists. The test `users_cannot_touch_each_others_tasks` in `tests/tasks.rs` checks this for GET, PATCH, DELETE and the list.

---

## 7. Errors: one shape for everything

Every error response has the same JSON shape:

```json
{ "error": "validation failed",
  "details": { "title": [ { "code": "length", "message": null, "params": { "min": 1, "max": 200, "value": "" } } ] } }
```

`details` is only present for 422 responses. The status codes follow one rule:

| Status | Meaning | Example |
|---|---|---|
| **400** | I couldn't *understand* the request | malformed JSON, `"priority": "urgent"`, unknown field, `/tasks/42`, `?sort=password_hash` |
| **401** | Who are you? | missing, invalid or expired token; bad login |
| **404** | Not found, *or not yours* | another user's task id |
| **405** | Path exists, method doesn't | `PUT /tasks` |
| **409** | Conflicts with existing data | email already registered |
| **422** | Understood, but the values break a rule | empty title, 5-character password |
| **500** | Our fault | database unreachable. Details are **logged**, never sent to the client. |

How it works:

- `error.rs` defines `AppError` and its `IntoResponse`. That's the only place that decides status codes and error bodies.
- Every layer returns `Result<_, AppError>` (repositories return `sqlx::Error`, which converts automatically through `#[from]`), so `?` works everywhere.
- axum's own `Json`, `Query` and `Path` extractors reply to bad input with **plain text**. `extractors.rs` wraps them as `AppJson`, `AppQuery` and `AppPath`, which convert those rejections into `AppError::BadRequest`. Handlers use these wrappers instead of axum's types.
- The router has fallbacks for unknown paths (404) and methods (405), so even those return JSON.
- Unexpected errors are logged **once**, inside `into_response`. They aren't also logged at every layer they pass through.

---

## 8. The database schema

```mermaid
erDiagram
    users ||--o{ tasks : "owns"

    users {
        BLOB id PK "UUID v7, 16 bytes"
        TEXT email UK "lowercased"
        TEXT password_hash "argon2id PHC string"
        TEXT created_at "RFC 3339, UTC"
    }

    tasks {
        BLOB id PK "UUID v7"
        BLOB user_id FK "→ users.id, ON DELETE CASCADE"
        TEXT title "1–200 chars, trimmed"
        TEXT description "nullable, ≤ 2000 chars"
        BOOLEAN done "default false"
        TEXT priority "low | medium | high (CHECK)"
        TEXT due_date "nullable, YYYY-MM-DD"
        TEXT created_at "RFC 3339, UTC"
        TEXT updated_at "RFC 3339, UTC"
    }
```

**Indexes:** `users.email` (unique, used by login), `tasks(user_id, created_at)` (default list order), `tasks(user_id, due_date)` (due-date filters and sorting). Every index starts with `user_id` because every task query filters by it.

**Migrations** (`migrations/*.sql`) are applied automatically at startup, in filename order. sqlx records the applied ones in a `_sqlx_migrations` table. The golden rule: **never edit a migration that has already run.** To change the schema, add a new file:

```powershell
cargo install sqlx-cli --no-default-features --features sqlite   # once
sqlx migrate add add_tags_to_tasks      # creates migrations/<timestamp>_add_tags_to_tasks.sql
```

Then restart the server. `build.rs` makes sure Cargo rebuilds so the new file is embedded.

**Why these types in SQLite:** SQLite has few native types. UUIDs are stored as 16-byte BLOBs (sqlx's encoding). Timestamps are ISO-8601 TEXT, which sorts correctly as text. `due_date` is a date without a time or time zone, because "due Friday" means Friday wherever you are. All instants (`created_at`, `updated_at`) are UTC.

**Settings applied per connection** (`db.rs`): `foreign_keys = ON` (SQLite ignores foreign keys without it!), WAL journal mode (reads don't block on writes), and a 5 s busy timeout (wait for the write lock instead of failing).

---

## 9. Filtering, sorting and pagination

```
GET /tasks?done=false&priority=high&due_before=2026-12-31&q=report&sort=due_date&order=asc&limit=10&offset=20
```

| Param | Values | Default |
|---|---|---|
| `done` | `true` / `false` | (any) |
| `priority` | `low` / `medium` / `high` | (any) |
| `due_before`, `due_after` | `YYYY-MM-DD` (inclusive) | (any). Undated tasks never match. |
| `q` | text, case-insensitive substring of title | (none) |
| `sort` | `created_at`, `updated_at`, `due_date`, `priority`, `title` | `created_at` |
| `order` | `asc` / `desc` | `desc` |
| `limit` | clamped to 1–100 | 20 |
| `offset` | clamped to ≥ 0 | 0 |

The response is an envelope: `{ "items": [...], "total": 42, "limit": 10, "offset": 20 }`. `total` counts all matches, so a client can compute `ceil(total / limit)` pages. `limit` and `offset` echo the values actually used after clamping.

How `repositories/tasks.rs` builds the query safely:

- **Filters are optional**, so the SQL is assembled with sqlx's `QueryBuilder`. Every *value* goes in through `push_bind` (a bind parameter), never pasted into the SQL text.
- **Sort columns can't be bind parameters** (SQL doesn't allow `ORDER BY ?`), so they must be written into the SQL text. That's why `sort` is an enum. serde rejects anything outside the five known names (`?sort=password_hash` is a 400), and `order_by()` maps each variant to a hard-coded string.
- **The count and the page share one `push_where` function**, so they can't disagree about which rows match.
- **Sort details:** priority sorts by rank (low < medium < high), not alphabetically. Undated tasks sort last in both directions. Titles sort case-insensitively. Every sort ends with `id` as a tie-breaker, so rows with equal values keep a stable order across pages. Without that, a task could appear on two pages or on none.
- **Search escapes `%` and `_`**, the LIKE wildcards, so searching `100%` finds the literal text "100%".
- **Unknown query parameters are rejected** (`deny_unknown_fields`). A typo like `?prority=high` returns a 400 instead of silently returning everything.

**About offset pagination:** it's simple and supports "jump to page 7", but `OFFSET 100000` makes the database walk past 100,000 rows, and inserts between page loads shift items across pages. Large feeds therefore use *cursor* (keyset) pagination: "give me 20 items after this one". For a personal task list, offset is the right trade-off.

---

## 10. Tests

The tests in `tests/` are **integration tests**: they send real HTTP requests to the real `Router` that `build_app()` returns, with the same routes, extractors, middleware and SQL as production.

- `tests/common/mod.rs`: `spawn_app()` creates a fresh **in-memory** SQLite database, runs the migrations and wraps the app in an `axum_test::TestServer`. Each test has its own database, so tests can run in parallel without seeing each other's data.
- `tests/auth.rs`: signup, login, token edge cases (expired, forged, malformed).
- `tests/tasks.rs`: CRUD, validation, mass assignment, cross-user access, JSON error shapes.
- `tests/task_list.rs`: every filter, sort and pagination rule.

Why this works: the app is a **library** (`lib.rs`), and `main.rs` is a thin wrapper around it. Test crates can only use what a library exports. They also never need a running server: `TestServer` calls the router in-process.

The in-memory pool is configured with exactly **one** connection that's never closed (`db::connect_in_memory`). Every SQLite `:memory:` connection is its own separate database, so with two connections a test could write to one and read from an empty other.

Argon2 is compiled with optimizations even in debug builds (`[profile.dev.package.argon2]` in `Cargo.toml`). Without that, each hash takes seconds and the suite crawls.

---

## 11. Moving to PostgreSQL later

The layering pays off here. Routes, extractors, services and models don't change. Almost all the work is in `db.rs`, `repositories/`, `migrations/` and the test setup.

### What changes

| Area | SQLite (now) | PostgreSQL | Where |
|---|---|---|---|
| Cargo feature | `sqlx/sqlite` | `sqlx/postgres` | `Cargo.toml` |
| Pool / DB types | `SqlitePool`, `Sqlite`, `SqliteConnectOptions` | `PgPool`, `Postgres`, `PgConnectOptions` | `db.rs`, `state.rs`, repositories, services |
| Placeholders | `?` | `$1`, `$2`, ... | every hand-written query (`QueryBuilder::push_bind` adapts automatically) |
| UUID column | `BLOB` | `UUID` | migrations |
| Timestamps | `TEXT` | `TIMESTAMPTZ` | migrations |
| `due_date` | `TEXT` | `DATE` | migrations |
| `priority` | `TEXT` + `CHECK` | `TEXT` + `CHECK`, or a native `CREATE TYPE priority AS ENUM (...)` (then add `#[sqlx(type_name = "priority")]` to the enum) | migrations, `models/task.rs` |
| Case-insensitive search | `LIKE` (case-insensitive for ASCII) | `ILIKE` (`LIKE` is case-*sensitive*) | `repositories/tasks.rs` |
| Case-insensitive sort | `title COLLATE NOCASE` | `lower(title)` (or an ICU collation) | `repositories/tasks.rs` |
| Undated-last sort | `due_date IS NULL, due_date` | `due_date ASC NULLS LAST` (native) | `repositories/tasks.rs` |
| Unique emails | `UNIQUE` on lowercased text | same (or the `citext` type) | migrations |
| Connection settings | `foreign_keys`, WAL, `busy_timeout` | none needed: FKs always on, real concurrent writers | `db.rs` |
| Test databases | in-memory, instant | a real server: `#[sqlx::test]` (creates a throwaway DB per test) or `testcontainers` (Postgres in Docker) | `tests/common` |
| `DATABASE_URL` | `sqlite://tasks.db?mode=rwc` | `postgres://user:pass@localhost:5432/tasks` | `.env` |

**Migrations** are database-specific SQL, so you'd start a fresh `migrations/` folder for Postgres rather than reuse the SQLite files. To move existing data, export and import it once. Small datasets can be scripted; for larger ones there are tools like `pgloader`.

### What stays the same

- **All the `?`-based error handling.** `sqlx::Error` is the same type, and `is_unique_violation()` works the same way on Postgres, so the 409 on duplicate emails needs no change.
- `FromRow` structs, `sqlx::Type` enums, `QueryBuilder`, transactions and `sqlx::migrate!`.
- Everything above the repositories.

### What gets *better*, and what you'd then revisit

- **Concurrent writes.** SQLite allows one writer at a time. Postgres handles many. You'd raise `max_connections` (for example to 10–20).
- **Lost updates.** `services::tasks::update` reads, modifies, then writes, so with two simultaneous PATCHes the later one wins. On Postgres you could lock the row while editing (`SELECT ... FOR UPDATE` inside a transaction), or add a `version` column for optimistic locking.
- **`RETURNING`.** Both databases support `INSERT ... RETURNING *`, which avoids a separate SELECT. It's a common Postgres idiom worth adopting.
- **Compile-time checked queries** (`sqlx::query_as!`) become more valuable, because the schema has more types to get wrong. They need a live database or `cargo sqlx prepare` at build time.

### Making the switch easier later

If you expect the move, two small habits help now:

1. Add `pub type Db = sqlx::Sqlite;` and `pub type DbPool = sqlx::Pool<Db>;` in `db.rs`, and use those aliases everywhere instead of `SqlitePool`. The switch then starts with changing two lines.
2. Keep SQLite-specific syntax (`COLLATE NOCASE`, `ESCAPE '\'`, `IS NULL` sorting) confined to `repositories/`, as it is now.

---

## 12. Where DEPENDENCIES.md's examples had to change

DEPENDENCIES.md's examples weren't compile-checked. These are the places where the code here differs from them, and why:

| DEPENDENCIES.md | Here | Why |
|---|---|---|
| `cargo add uuid@1 --features v4,v7,serde` | Added to `Cargo.toml` by hand | `cargo add` refused with "unrecognized feature v7", even though uuid 1.29 has it (a `cargo add` quirk, not a uuid problem). The line itself is correct. |
| `argon2@0.5` with default features | `argon2 = { version = "0.5", features = ["std"] }` | `password_hash::rand_core::OsRng` is gated behind rand_core's `getrandom` feature, which argon2's defaults don't enable. Compile error: *no `OsRng` in the root*. |
| `TimeoutLayer::new(duration)` | `TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, duration)` | `new` is deprecated since tower-http 0.6.7. |
| `axum@0.8` | `axum` with the `macros` feature | Needed for `#[derive(FromRequest)]` on the `AppJson`/`AppQuery`/`AppPath` wrappers. |
| `AppError` variants | Added `BadRequest(String)` and `MethodNotAllowed` | Consistent JSON errors for parse failures and wrong methods (axum's defaults are plain text or empty). |
| `Unauthorized` message "invalid credentials" | "invalid or missing credentials" | The same variant is also used for a missing token. |
| `hash_password` / `verify_password` return `anyhow::Result` and use `.await?` on the `JoinHandle` | Return `Result<_, AppError>`; the `JoinError` is converted explicitly | `?` can't convert a `JoinError` straight into `AppError`, and the services need `AppError`. `verify_password` also takes `Option<String>` so it can fall back to the dummy hash (the "Going further" note in DEPENDENCIES.md, now implemented). |
| `login(state: &AppState, ...)` | `login(db, config, ...)` | Services take only what they use. |
| `Claims { sub, exp }` | Added `iat` (issued-at) | Standard claim, useful when debugging tokens. |
| `ListParams` clamped in the handler | Clamping and filter cleanup in `services::tasks::list` | That's a business rule, so it belongs in the service. |
| Migration `CREATE TABLE tasks` before `users` | `users` first, then `tasks` (with `priority`, `due_date`, `updated_at`) | `tasks` references `users`. |

The rest (the `AuthUser` extractor, `spawn_blocking` hashing, `AppError`'s 500 handling, `Config::from_env`, the tracing setup) works as written.

---

## 13. Ideas for what to build next

Roughly in order of difficulty, each one exercising a different part of the stack:

1. **Change password** endpoint (`PATCH /auth/me/password`): service rules plus re-hashing.
2. **Tags** on tasks: a many-to-many table, a transaction, and avoiding N+1 queries (DEPENDENCIES.md §5).
3. **Rate-limit login** with `tower_governor`: middleware on a single route.
4. **OpenAPI docs** with `utoipa`: generated from your types.
5. **Refresh tokens** stored in the DB, so you get JWT's statelessness *and* a real logout.
6. **Switch to PostgreSQL** following §11.
