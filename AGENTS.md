# AGENTS.md

## Quick start

```bash
./run.sh              # start Postgres (Docker) + server
docker compose up -d  # Postgres only
cargo run             # server on 0.0.0.0:3000
cargo build           # compile
cargo test            # all tests
```

## Required env

`DATABASE_URL`, `JWT_SECRET`, `JWT_EXPIRE` (seconds). A local `.env` exists but is gitignored — create it or set env vars. `init.sql` auto-runs on first Docker start.

## Architecture

Single crate `myx-im` (edition 2024). See `CLAUDE.md` for detailed layering (dao → state → service → router → main). Bottom line:

- **Stack:** axum 0.8 (HTTP + WS), sqlx (PostgreSQL), jsonwebtoken 10.x, bcrypt, tokio
- **Routes:** `GET /` (embedded `chat.html`), `GET /im/ws?token=` (WS), `POST /api/user/register`, `POST /api/user/login`
- **WS protocol:** JSON `{cmd, seq, data}` — `heartbeat` (no-op), `private_chat` (DB save + push to online recipient)
- **User IDs:** UUID v4 everywhere

## Gotchas

- `chat.html` is embedded at compile time via `include_str!` — edit the file and rebuild
- `.sqlx/` is committed for offline query checking. If you add/modify queries, run `cargo sqlx prepare` against a live DB
- `examples/` is a separate Cargo workspace, not part of the main crate
- No CI, no rustfmt/clippy config — standard `cargo` tooling only
