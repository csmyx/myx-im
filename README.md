# myx-im

IM system written by Rust.

## Prerequisites

- Rust (edition 2024)
- Docker (for local testing database)

## Quick start (testing)

```bash
# 1. Start PostgreSQL 16
docker compose up -d

# 2. Run the server
cargo run

# 3. Open the test page
open http://localhost:3000
```

## Reset database

```bash
docker compose down -v   # wipe data
docker compose up -d     # fresh start
```
