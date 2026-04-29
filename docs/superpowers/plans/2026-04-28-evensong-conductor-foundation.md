# Evensong-Conductor Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first durable Evensong-Conductor foundation: Supabase ledger schema, Rust core scaffold, and docs that connect the upstream Symphony runner to the new state layer.

**Architecture:** Keep upstream Symphony Elixir untouched as the reference runner. Add Evensong-specific foundation files in `supabase/`, `crates/conductor-core/`, and `docs/`. The first executable proof is a Rust health command that connects to Postgres and writes a redacted event.

**Tech Stack:** Rust, SQLx, Supabase Postgres, existing Elixir Symphony reference runner, GitHub fork workflow.

---

## Scope

This plan implements only the foundation slice from `docs/superpowers/specs/2026-04-28-evensong-conductor-design.md`.

Included:

- Supabase migration for core conductor tables and indexes.
- Rust workspace with one `conductor-core` crate.
- A small CLI command that checks database connectivity and writes one synthetic event.
- Documentation explaining local setup and boundaries.

Excluded:

- Operator console UI.
- Capy adapter.
- Hermes/MiMo worker execution.
- Replacement of the Elixir scheduler.
- Production deployment.

## File Structure

- Create: `Cargo.toml`
  - Workspace manifest for Rust crates.
- Create: `crates/conductor-core/Cargo.toml`
  - Rust crate manifest.
- Create: `crates/conductor-core/src/lib.rs`
  - Public module exports.
- Create: `crates/conductor-core/src/config.rs`
  - Reads `DATABASE_URL` and validates runtime config.
- Create: `crates/conductor-core/src/ledger.rs`
  - SQLx-backed ledger functions.
- Create: `crates/conductor-core/src/main.rs`
  - `conductor-core ledger-health` CLI.
- Create: `supabase/migrations/20260428180000_conductor_core.sql`
  - Core table schema and indexes.
- Create: `docs/EVENSONG-CONDUCTOR.md`
  - Human-readable architecture and local run guide.
- Modify: `README.md`
  - Add a short fork note pointing to Evensong-Conductor docs.
- Test: `cargo test`
- Test: `supabase db reset` after local project initialization.

## Task 1: Add The Supabase Ledger Migration

**Files:**

- Create: `supabase/migrations/20260428180000_conductor_core.sql`

- [ ] **Step 1: Write the migration**

Create `supabase/migrations/20260428180000_conductor_core.sql` with this content:

```sql
create extension if not exists pgcrypto;

create table conductor_projects (
  id uuid primary key default gen_random_uuid(),
  slug text unique not null,
  name text not null,
  repo_url text not null,
  default_branch text not null default 'main',
  created_at timestamptz not null default now()
);

create table conductor_work_items (
  id uuid primary key default gen_random_uuid(),
  project_id uuid not null references conductor_projects(id) on delete cascade,
  source_kind text not null,
  source_id text not null,
  source_identifier text not null,
  source_url text,
  title text not null,
  state text not null,
  priority int,
  labels text[] not null default '{}',
  payload_redacted jsonb not null default '{}',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  unique (source_kind, source_id)
);

create index conductor_work_items_project_state_priority_idx
  on conductor_work_items (project_id, state, priority, updated_at desc);

create index conductor_work_items_labels_idx
  on conductor_work_items using gin (labels);

create table conductor_runs (
  id uuid primary key default gen_random_uuid(),
  work_item_id uuid not null references conductor_work_items(id) on delete cascade,
  status text not null,
  policy_hash text not null,
  workspace_key text not null,
  started_at timestamptz not null default now(),
  ended_at timestamptz,
  failure_reason text
);

create index conductor_runs_work_item_started_idx
  on conductor_runs (work_item_id, started_at desc);

create index conductor_runs_status_started_idx
  on conductor_runs (status, started_at desc);

create table conductor_run_attempts (
  id uuid primary key default gen_random_uuid(),
  run_id uuid not null references conductor_runs(id) on delete cascade,
  attempt_number int not null,
  worker_kind text not null,
  worker_id text,
  model text,
  status text not null,
  started_at timestamptz not null default now(),
  ended_at timestamptz,
  error_summary text,
  unique (run_id, attempt_number)
);

create table conductor_workers (
  id uuid primary key default gen_random_uuid(),
  name text unique not null,
  kind text not null,
  host_label text not null,
  capabilities text[] not null default '{}',
  status text not null default 'unknown',
  last_heartbeat_at timestamptz
);

create index conductor_workers_status_heartbeat_idx
  on conductor_workers (status, last_heartbeat_at desc);

create index conductor_workers_capabilities_idx
  on conductor_workers using gin (capabilities);

create table conductor_worker_leases (
  id uuid primary key default gen_random_uuid(),
  worker_id uuid not null references conductor_workers(id) on delete cascade,
  run_attempt_id uuid references conductor_run_attempts(id) on delete set null,
  lease_key text not null,
  expires_at timestamptz not null,
  created_at timestamptz not null default now()
);

create index conductor_worker_leases_key_expiry_idx
  on conductor_worker_leases (lease_key, expires_at);

create index conductor_worker_leases_worker_expiry_idx
  on conductor_worker_leases (worker_id, expires_at);

create table conductor_events (
  id bigserial primary key,
  run_id uuid references conductor_runs(id) on delete cascade,
  run_attempt_id uuid references conductor_run_attempts(id) on delete cascade,
  event_type text not null,
  severity text not null default 'info',
  message text not null,
  payload_redacted jsonb not null default '{}',
  created_at timestamptz not null default now()
);

create index conductor_events_run_id_idx
  on conductor_events (run_id, id);

create index conductor_events_run_attempt_id_idx
  on conductor_events (run_attempt_id, id);

create index conductor_events_type_created_idx
  on conductor_events (event_type, created_at desc);

create table conductor_artifacts (
  id uuid primary key default gen_random_uuid(),
  run_id uuid references conductor_runs(id) on delete cascade,
  kind text not null,
  label text not null,
  uri text not null,
  sha256 text,
  redaction_level text not null default 'public-safe',
  created_at timestamptz not null default now()
);

create index conductor_artifacts_run_kind_created_idx
  on conductor_artifacts (run_id, kind, created_at desc);

create table conductor_model_usage (
  id uuid primary key default gen_random_uuid(),
  run_attempt_id uuid not null references conductor_run_attempts(id) on delete cascade,
  provider text not null,
  model text not null,
  input_tokens bigint not null default 0,
  output_tokens bigint not null default 0,
  total_tokens bigint not null default 0,
  context_window bigint,
  recorded_at timestamptz not null default now()
);

create index conductor_model_usage_attempt_recorded_idx
  on conductor_model_usage (run_attempt_id, recorded_at desc);

create index conductor_model_usage_provider_model_recorded_idx
  on conductor_model_usage (provider, model, recorded_at desc);
```

- [ ] **Step 2: Verify migration syntax**

Run:

```bash
supabase start
supabase db reset
```

Expected:

- Supabase local stack starts.
- Migration applies with no SQL syntax errors.
- No secrets are printed into committed files.

- [ ] **Step 3: Commit**

Run:

```bash
git add supabase/migrations/20260428180000_conductor_core.sql
git commit -m "feat(db): add conductor ledger schema"
```

## Task 2: Add The Rust Workspace

**Files:**

- Create: `Cargo.toml`
- Create: `crates/conductor-core/Cargo.toml`
- Create: `crates/conductor-core/src/lib.rs`
- Create: `crates/conductor-core/src/config.rs`

- [ ] **Step 1: Create the workspace manifest**

Create `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
  "crates/conductor-core"
]
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/conductor-core/Cargo.toml`:

```toml
[package]
name = "conductor-core"
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }

[dev-dependencies]
temp-env = "0.3"
```

- [ ] **Step 3: Create module exports**

Create `crates/conductor-core/src/lib.rs`:

```rust
pub mod config;
pub mod ledger;
```

- [ ] **Step 4: Create runtime config**

Create `crates/conductor-core/src/config.rs`:

```rust
use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConductorConfig {
    pub database_url: String,
}

impl ConductorConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must point at the conductor Postgres database")?;

        Ok(Self { database_url })
    }
}

#[cfg(test)]
mod tests {
    use super::ConductorConfig;

    #[test]
    fn reads_database_url_from_environment() {
        temp_env::with_var(
            "DATABASE_URL",
            Some("postgres://postgres:postgres@127.0.0.1:54322/postgres"),
            || {
                let config = ConductorConfig::from_env().expect("config should load");
                assert_eq!(
                    config.database_url,
                    "postgres://postgres:postgres@127.0.0.1:54322/postgres"
                );
            },
        );
    }

    #[test]
    fn errors_when_database_url_is_missing() {
        temp_env::with_var("DATABASE_URL", Option::<&str>::None, || {
            let error = ConductorConfig::from_env().expect_err("missing env should fail");
            assert!(error.to_string().contains("DATABASE_URL"));
        });
    }
}
```

- [ ] **Step 5: Verify Rust tests fail before ledger exists only if imports are wrong**

Run:

```bash
cargo test -p conductor-core config
```

Expected:

- `reads_database_url_from_environment` passes.
- `errors_when_database_url_is_missing` passes.

- [ ] **Step 6: Commit**

Run:

```bash
git add Cargo.toml crates/conductor-core
git commit -m "feat(core): add conductor rust workspace"
```

## Task 3: Add Ledger Health Write

**Files:**

- Create: `crates/conductor-core/src/ledger.rs`
- Create: `crates/conductor-core/src/main.rs`

- [ ] **Step 1: Create ledger functions**

Create `crates/conductor-core/src/ledger.rs`:

```rust
use anyhow::Result;
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::ConductorConfig;

pub async fn connect(config: &ConductorConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    Ok(pool)
}

pub async fn write_health_event(pool: &PgPool) -> Result<i64> {
    let record = sqlx::query!(
        r#"
        insert into conductor_events (event_type, severity, message, payload_redacted)
        values ($1, $2, $3, $4)
        returning id
        "#,
        "ledger.health",
        "info",
        "conductor ledger health check",
        json!({"source": "conductor-core"})
    )
    .fetch_one(pool)
    .await?;

    Ok(record.id)
}
```

- [ ] **Step 2: Create the CLI**

Create `crates/conductor-core/src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use conductor_core::{config::ConductorConfig, ledger};

#[derive(Debug, Parser)]
#[command(name = "conductor-core")]
#[command(about = "Evensong-Conductor core utilities")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    LedgerHealth,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::LedgerHealth => {
            let config = ConductorConfig::from_env()?;
            let pool = ledger::connect(&config).await?;
            let event_id = ledger::write_health_event(&pool).await?;
            println!("ledger health event written: {event_id}");
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Run database-backed smoke test**

Run:

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:54322/postgres"
cargo run -p conductor-core -- ledger-health
```

Expected:

- Command prints `ledger health event written: <number>`.
- A row exists in `conductor_events` with `event_type = 'ledger.health'`.

- [ ] **Step 4: Commit**

Run:

```bash
git add crates/conductor-core/src/ledger.rs crates/conductor-core/src/main.rs
git commit -m "feat(core): add ledger health command"
```

## Task 4: Add Operator Documentation

**Files:**

- Create: `docs/EVENSONG-CONDUCTOR.md`
- Modify: `README.md`

- [ ] **Step 1: Create docs**

Create `docs/EVENSONG-CONDUCTOR.md`:

```md
# Evensong-Conductor

Evensong-Conductor is the Evensong operations layer built on top of the OpenAI Symphony specification.

It keeps the upstream Elixir runner as a reference implementation while adding a durable Supabase ledger, a Rust core, and adapters for the agent systems used in the Evensong workflow.

## Current Layers

- Upstream Symphony spec: `SPEC.md`
- Upstream Elixir reference runner: `elixir/`
- Evensong design spec: `docs/superpowers/specs/2026-04-28-evensong-conductor-design.md`
- Foundation plan: `docs/superpowers/plans/2026-04-28-evensong-conductor-foundation.md`
- Supabase migrations: `supabase/migrations/`
- Rust core: `crates/conductor-core/`

## Local Database

Start Supabase locally:

```bash
supabase start
supabase db reset
```

Use the local database URL:

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:54322/postgres"
```

Run the ledger smoke check:

```bash
cargo run -p conductor-core -- ledger-health
```

## Boundary

The ledger stores redacted operational facts, not secrets. Keep API keys, private endpoints, local absolute paths, and raw private terminal logs out of committed files and out of ledger payloads.
```

- [ ] **Step 2: Update README**

Add this section after the warning block in `README.md`:

```md
## Evensong-Conductor Fork

This fork is becoming Evensong-Conductor: an agent operations layer for Linear/GitHub work, Codex/Hermes/MiMo worker sessions, GSD/Superpowers policy checks, and Research Vault-backed memory.

The upstream Symphony spec and Elixir reference runner remain intact. Evensong-specific design and implementation notes live in [`docs/EVENSONG-CONDUCTOR.md`](docs/EVENSONG-CONDUCTOR.md).
```

- [ ] **Step 3: Verify docs do not leak local/private details**

Run:

```bash
rg -n "100\\.|/Users/|Tailscale|API_KEY|SECRET|TOKEN|PRIVATE|localhost:8765" README.md docs supabase crates || true
```

Expected:

- No private IPs.
- No local absolute paths.
- No raw secret-shaped strings.
- `DATABASE_URL` examples are local Supabase defaults only.

- [ ] **Step 4: Commit**

Run:

```bash
git add README.md docs/EVENSONG-CONDUCTOR.md
git commit -m "docs: describe evensong conductor foundation"
```

## Task 5: Final Verification

**Files:**

- Verify all files touched by Tasks 1-4.

- [ ] **Step 1: Run formatting and tests**

Run:

```bash
cargo fmt
cargo test
```

Expected:

- Rust formatting completes.
- Rust tests pass.

- [ ] **Step 2: Run Supabase migration reset**

Run:

```bash
supabase db reset
```

Expected:

- All migrations apply.

- [ ] **Step 3: Run the ledger command**

Run:

```bash
export DATABASE_URL="postgres://postgres:postgres@127.0.0.1:54322/postgres"
cargo run -p conductor-core -- ledger-health
```

Expected:

- A `ledger.health` event ID is printed.

- [ ] **Step 4: Check repository status**

Run:

```bash
git status --short
git log --oneline --max-count=5
```

Expected:

- Working tree is clean.
- Recent commits match the foundation tasks.

## Plan Self-Review

- The plan implements only the foundation slice.
- Each task has exact files, commands, and expected outcomes.
- Supabase schema uses targeted indexes and append-only events.
- Worker execution is not added before durable state exists.
- Elixir reference behavior remains untouched.
- No placeholder sections remain.
