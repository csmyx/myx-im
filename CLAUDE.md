# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Quick start

```bash
./run.sh                 # starts PostgreSQL via Docker, then runs the server
# or manually:
docker compose up -d     # start PostgreSQL 16
cargo run                # start server on 0.0.0.0:3000
cargo build              # compile only
cargo test               # run all tests
cargo test -p myx-im     # run only the main crate's tests
```

Requires PostgreSQL. Set `DATABASE_URL`, `JWT_SECRET`, and `JWT_EXPIRE` (seconds) either via `.env` or environment variables. The `init.sql` creates tables automatically on first Docker start.

## Architecture

An IM (instant messaging) system in early development. Many planned routes are commented out in `router.rs`.

**Stack:** axum 0.8 (HTTP + WebSocket), sqlx (PostgreSQL), jsonwebtoken 10.x (JWT, requires `rust_crypto` feature), bcrypt, tokio.

**Layering (bottom-up):**

1. **`src/dao.rs`** — Raw SQL via `sqlx::query!` / `sqlx::query` with `PgPool`. Tables: `im_users` (id UUID, username, password_hash, created_at), `im_chat_messages` (id BIGSERIAL, from_uid UUID, to_uid UUID, content, msg_type SMALLINT, created_at). Returns `anyhow::Result`.

2. **`src/state.rs`** — `AppState` holds `PgPool` (pub), `Config` (pub), and `Arc<Mutex<HashMap<Uuid, OnlineUser>>>` (private). Methods: `insert_online_user(uid, tx)` and `send_to_user(uid, msg)` for real-time delivery via `mpsc::UnboundedSender<Utf8Bytes>`. Dead channels are auto-cleaned.

3. **`src/service.rs`** — Business logic for `register_user(pool, config, username, password)` and `login_user(pool, config, username, password)`. Handles bcrypt hashing/verification, DAO calls, JWT creation, and error classification (unique constraint → 409, wrong password → 401, etc.).

4. **`src/router.rs`** — All route handlers and the `app_router(state) -> Router` builder. WS message dispatch (`handle_biz_msg`) lives here, calling `dao::save_message` and `state.send_to_user` directly. `WsQuery { token }` is the WS auth query param.

5. **`src/main.rs`** — Thin entrypoint: tracing init, pool creation, `init_app_state`, `app_router`, bind & serve.

6. **`src/model.rs`** — Structs: `User`, `RegisterRequest`, `LoginRequest`, `Res<T>` (unified response with `code`, `msg`, `data`), `WsMessage { cmd, seq, data }`, `PrivateChatReq { to_uid: Uuid, content, msg_type, extra }`, `PrivatePushMsg`, `GroupChatReq`, `GroupPushMsg`. Uses `validator::Validate` derive.

7. **`src/jwt.rs`** — `create_token(user_id: Uuid, config)` / `verify_token(token, config)` using HS256. `Claims { user_id: Uuid, exp, iat }`.

8. **`src/config.rs`** — Loads `DATABASE_URL`, `JWT_SECRET`, `JWT_EXPIRE` from env via `dotenv`.

**WebSocket IM protocol:** Clients connect at `/im/ws?token=<JWT>`. Messages are JSON `{cmd, seq, data}`. Implemented commands: `"heartbeat"` (no-op), `"private_chat"` (saves to DB + pushes to recipient if online). `msg_type`: 1 = text.

**Active routes (in `router.rs`):**
- `GET /` — serves `chat.html` (two-user test UI)
- `GET /im/ws?token=...` — WebSocket upgrade with JWT auth
- `POST /api/user/register` — registration
- `POST /api/user/login` — login (returns JWT)

**User IDs:** UUID v4 throughout (DB columns, JWT claims, WS routing, online-users map).

**Testing frontend:** `chat.html` at `GET /` has side-by-side Alice/Bob panels pre-filled with test credentials. Register both users, then Login & Connect on each to auto-connect WS and chat bidirectionally.

**Key patterns:**
- Handlers in `router.rs` call `service::register_user`/`service::login_user` for user operations, and `dao::save_message` + `state.send_to_user` directly for messaging.
- `AppState.pg_pool` and `.config` are public — service functions take them individually rather than taking the whole `AppState`.
- The `examples/` directory is standalone reference code, not part of the main binary.
