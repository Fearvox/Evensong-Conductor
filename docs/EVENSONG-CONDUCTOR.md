# Evensong-Conductor

Evensong-Conductor is the Evensong operations layer built on top of the OpenAI Symphony specification.

It keeps the upstream Elixir runner as a reference implementation while adding a durable Supabase ledger, a Rust core, and adapters for the agent systems used in the Evensong workflow.

## Current Layers

- Upstream Symphony spec: `SPEC.md`
- Upstream Elixir reference runner: `elixir/`
- Evensong design spec: `docs/superpowers/specs/2026-04-28-evensong-conductor-design.md`
- Foundation plan: `docs/superpowers/plans/2026-04-28-evensong-conductor-foundation.md`
- Operator console v0 plan: `docs/superpowers/plans/2026-04-28-operator-console-v0.md`
- Supabase migrations: `supabase/migrations/`
- Rust core: `crates/conductor-core/`

## Local Database

The easiest first run is:

```bash
make first-launch
```

That command checks for a Docker daemon, starts Colima when available, starts the Supabase local stack, applies migrations, runs Rust tests, and writes one redacted ledger health event. It skips Supabase's optional analytics/log collector containers because the conductor ledger does not need them and Colima can reject the Docker socket mount those containers use.

The migration enables RLS on all conductor tables and revokes direct `anon` / `authenticated` access. The current CLI path uses the local Postgres connection for owner-side ledger writes; future public or app-facing APIs should add explicit policies instead of inheriting broad default access.

If you want to run the steps manually, start Supabase locally:

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

Start the local operator console:

```bash
make console
```

Then open:

```text
http://127.0.0.1:4317
```

The console reads the local ledger, shows table counts, recent redacted events, and the current RLS/public-grant posture. It does not render the database URL, private prompts, API keys, or raw terminal logs.

Supabase Studio is available at:

```text
http://127.0.0.1:54323
```

## Remote Hermes Supervision

The first real worker adapter is a narrow Hermes supervisor probe. It checks a remote SSH target, validates a tmux pane, captures only enough state to determine health, and can optionally send a one-line smoke ping.

Set the target at runtime instead of committing private host details:

```bash
export HERMES_SSH_TARGET="user@host"
export HERMES_TMUX_TARGET="session:window"
```

Read-only probe:

```bash
make hermes-health
```

Explicit smoke probe, with a redacted `hermes.health` event written into the local ledger:

```bash
make hermes-smoke
```

The ledger event stores redacted operational facts such as status, remote host label, tmux target, memory availability, and smoke result. It does not store SSH target strings, raw terminal capture, private prompts, API keys, or local paths.

## Boundary

The ledger stores redacted operational facts, not secrets. Keep API keys, private endpoints, local absolute paths, and raw private terminal logs out of committed files and out of ledger payloads.
