#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:54322/postgres}"

echo "== Evensong-Conductor first launch =="
echo "repo: $ROOT_DIR"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker CLI is missing. Install Docker Desktop or Colima before continuing." >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  if command -v colima >/dev/null 2>&1; then
    echo "docker daemon is not running; starting Colima..."
    colima start
  else
    echo "docker daemon is not running, and colima is unavailable." >&2
    echo "Start Docker Desktop, then rerun: make first-launch" >&2
    exit 1
  fi
fi

echo "starting Supabase local stack..."
# Colima cannot mount its forwarded docker.sock into Supabase's vector log
# container on some macOS/VZ setups. The conductor ledger only needs Postgres,
# Studio, and API services, so skip the optional analytics containers.
supabase start --exclude vector,logflare

echo "resetting local database and applying migrations..."
supabase db reset

echo "running Rust tests..."
cargo test

echo "writing ledger health event..."
DATABASE_URL="$DATABASE_URL" cargo run -p conductor-core -- ledger-health

echo
echo "first launch ready"
echo "Supabase Studio: http://127.0.0.1:54323"
echo "Postgres URL:    $DATABASE_URL"
echo "Next check:      DATABASE_URL=\"$DATABASE_URL\" cargo run -p conductor-core -- ledger-health"
