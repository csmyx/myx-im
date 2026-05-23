# Copilot instructions for myx-im

Purpose: provide concise guidance for future Copilot CLI sessions and contributors about building, testing, and repository-level conventions.

---

## Build, test, and lint commands

- Quick local dev (Postgres required):

```bash
./run.sh              # start Postgres (Docker) + server
# or start DB only:
docker compose up -d
# then run server:
cargo run
```

- Build:

```bash
cargo build
```

- Run full test suite:

```bash
cargo test
# or run only the main crate tests:
cargo test -p myx-im
```

- Run a single test (by name pattern):

```bash
cargo test <TEST_NAME_PATTERN>
# examples:
# cargo test login_success
# cargo test my_module::tests::test_name
# or limit to package:
# cargo test -p myx-im <TEST_NAME_PATTERN>
```

- sqlx offline checks:

```bash
# .sqlx/ is committed for offline checking. If you add/modify queries:
# ensure a live DB and run:
cargo sqlx prepare -- --database-url "$DATABASE_URL"
```

- Formatting / linting:

```bash
cargo fmt
cargo clippy --all-targets --all-features
```

Notes:
- Tests that touch the database require a running Postgres instance (use `docker compose up -d`).
- Required env vars: DATABASE_URL, JWT_SECRET, JWT_EXPIRE (seconds). A local `.env` is used by the project but is gitignored.

---

## High-level architecture (big picture)

This repository implements a single-crate IM server `myx-im`. Key layers (bottom-up):

- src/dao.rs      — raw SQL access via `sqlx::query!` / `sqlx::query` (DB schema: `im_users`, `im_chat_messages`).
- src/state.rs    — `AppState` holds PgPool, Config, and an in-memory online-users map with mpsc senders for WebSocket delivery.
- src/service.rs  — business logic for user registration/login (bcrypt, JWT creation). Returns domain errors mapped to HTTP codes.
- src/router.rs   — HTTP + WebSocket handlers and `app_router(state)` builder. WS dispatch (`handle_biz_msg`) saves messages via DAO and uses state to push to recipients.
- src/main.rs     — sets up tracing, DB pool, AppState, and starts the server.
- src/model.rs    — shared types: request/response wrappers, WS message shapes, PrivateChatReq, Push messages, etc.
- src/jwt.rs      — create/verify JWT (HS256, `Claims { user_id, exp, iat }`).
- src/config.rs   — loads env vars via dotenv.

Runtime details:
- WebSocket endpoint: `GET /im/ws?token=<JWT>`
- HTTP routes: `GET /` (serves embedded `chat.html`), `POST /api/user/register`, `POST /api/user/login`.
- WS protocol: JSON messages of shape `{cmd, seq, data}`. Implemented commands: `heartbeat` (noop), `private_chat` (DB save + push to recipient). `msg_type` uses `1` for text.
- Client test UI: `chat.html` is embedded at compile time via `include_str!` — editing requires rebuild.

---

## Key conventions and repository patterns

- UUID v4 for all user IDs across DB, JWT claims, and in-memory maps.
- Unified API response type `Res<T>` (fields: code, msg, data) used by handlers.
- Handler pattern:
  - User register/login go through `service::{register_user, login_user}`.
  - Messaging uses `dao::save_message` + `state.send_to_user` directly inside WS handler.
- AppState design:
  - `AppState.pg_pool` and `AppState.config` are public; service functions accept pool + config rather than the whole state.
  - `state` contains a `HashMap<Uuid, OnlineUser>` guarded by Arc<Mutex<...>> and methods to insert/remove/send.
- Database queries:
  - `.sqlx/` folder is committed for offline checking. After changing queries, run `cargo sqlx prepare` against a live DB.
  - `init.sql` is used by the Docker startup scripts to initialize schema on first run.
- Examples:
  - `examples/` is a separate cargo workspace for sample clients and is not part of the main binary.
- Environment:
  - Required env vars: `DATABASE_URL`, `JWT_SECRET`, `JWT_EXPIRE` (seconds). Use `.env` locally or set env vars in CI/dev shells.
- No repo-level CI or rustfmt/clippy config is provided; prefer `cargo fmt` and `cargo clippy` locally.

---

## Files to check first when changing behavior

- `src/router.rs` — route wiring and WS handling
- `src/service.rs` — authentication logic and JWT creation
- `src/dao.rs` — SQL queries and DB interaction
- `src/state.rs` — online users and in-memory messaging
- `chat.html` / `examples/` — small frontend test harness (editing requires rebuild)

---

If the file `.github/copilot-instructions.md` already existed, this file aims to merge and consolidate README.md / AGENTS.md / CLAUDE.md guidance into a single place for Copilot sessions.

