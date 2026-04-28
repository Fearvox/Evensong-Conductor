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
