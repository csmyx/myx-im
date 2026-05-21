#!/usr/bin/env bash
set -e

# Start PostgreSQL if not running
if ! docker ps --format '{{.Names}}' | grep -q '^myx-im-db$'; then
    echo "Starting PostgreSQL..."
    docker compose up -d
    echo "Waiting for PostgreSQL to be ready..."
    until docker exec myx-im-db pg_isready -U postgres >/dev/null 2>&1; do
        sleep 0.5
    done
    echo "PostgreSQL is ready."
fi

RUST_BACKTRACE=1 cargo run
