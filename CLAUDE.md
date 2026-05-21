# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and run

```bash
cargo build              # compile
cargo run                # start server on 0.0.0.0:3000
cargo test               # run all tests
cargo test -p myx-im     # run only the main crate's tests
```

Requires a running PostgreSQL instance. Set `DATABASE_URL`, `JWT_SECRET`, and `JWT_EXPIRE` (seconds) either via `.env` or environment variables.

## Architecture

This is an IM (instant messaging) system. The project is in early development — many planned routes are commented out.

**Current stack:** axum (HTTP + WebSocket), sqlx (PostgreSQL via `PgPool`), jsonwebtoken (JWT auth), bcrypt (password hashing), tokio (async runtime).

**Layering (bottom-up):**

1. **`src/dao.rs`** — Raw SQL queries via `sqlx::query!` / `sqlx::query_as!`. Functions take `&PgPool` directly and return `anyhow::Result`. Tables: `im_users` (id UUID, username, password_hash, created_at), `im_chat_messages` (from_uid, to_uid, content, msg_type).

2. **`src/state.rs`** — `AppState` wraps `PgPool` + `Arc<Mutex<HashMap<u64, OnlineUser>>>` (online user connections) + `Config`. It re-exposes DAO functions as methods (delegating to the pool). Each online user is an `mpsc::UnboundedSender<Utf8Bytes>` — messages are pushed to the user's channel and a spawned task writes them to the WebSocket. Also provides `send_to_user()` for real-time message delivery; if the channel is dead, the user is auto-removed from the map.

3. **`src/main.rs`** — All HTTP/WS handlers live directly in main. The `Router` is built with `AppState` as shared state (`Arc<AppState>`). No separate router or service modules are currently active (`router.rs` and `service.rs` are entirely commented-out legacy code).

4. **`src/model.rs`** — Request/response structs (`RegisterRequest`, `LoginRequest`, `Res<T>` unified response, `WsMessage`, `PrivateChatReq`, `PrivatePushMsg`, etc.). Uses `validator::Validate` derive on input structs.

5. **`src/jwt.rs`** — `create_token(user_id: Uuid)` and `verify_token(token)` using HS256. JWT secret and expiry come from `Config`.

6. **`src/config.rs`** — Loads from env vars (via `dotenv`): `DATABASE_URL`, `JWT_SECRET`, `JWT_EXPIRE`.

**WebSocket IM protocol:** Clients connect at `/im/ws/{uid}`. Messages are JSON with `{cmd, seq, data}`. Currently implemented commands: `"heartbeat"` (no-op), `"private_chat"` (saves to DB + pushes to recipient if online). `msg_type`: 1 = text, 2 = planned for other media.

**Active routes:**
- `GET /` — serves `chat.html`
- `GET /im/ws/{uid}` — WebSocket upgrade
- `POST /api/user/register` — registration
- `POST /api/user/login` — login (returns JWT)

**Key patterns:**
- Handlers in main.rs directly call `state.method()` — no service layer indirection.
- `AppState` is the integration point: it owns the pool, the online-users map, and config, so handlers never touch these individually.
- Old code in `router.rs` and `service.rs` used a different pattern (separate HTTP/WS routers, global token map, `lazy_static`) — this is dead code and should not be referenced or revived without explicit direction.
- The `examples/` directory contains standalone axum/websocket reference code (not part of the main binary).
