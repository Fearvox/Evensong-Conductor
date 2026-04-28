# Operator Console v0 Spec

Date: 2026-04-28
Status: approved for implementation
Parent spec: `2026-04-28-evensong-conductor-design.md`

## Goal

Ship the first real GUI surface for Evensong-Conductor.

This is not the final orchestration app. It is the operator-facing console that proves the durable ledger is visible from a browser and can support the next worker/session loop.

## User Need

The operator should be able to open one local URL and answer:

- is the conductor database reachable?
- how many durable objects exist in each ledger table?
- did recent health or worker events land?
- are the ledger privacy/security guardrails still true?
- what command starts the next layer?

## Scope

Build an additive Rust HTTP console inside `conductor-core`.

The console must:

- read the existing Supabase Postgres ledger
- render a clean HTML dashboard at `/`
- expose machine-readable health at `/healthz`
- allow a manual redacted health event write from the page
- show table counts, recent redacted events, RLS status, and public-role grant status
- keep secrets and local database URLs out of rendered HTML

## Non-Goals

- no Elixir runner rewrite
- no authenticated multi-user web app yet
- no public deployment yet
- no raw prompt/log rendering
- no Linear/GitHub/Capy sync yet

## Design Direction

Visual style:

- quiet light mode by default
- Helvetica/SF-style system typography
- white panels, thin dividers, compact spacing
- dark mode through `prefers-color-scheme`
- no grid background, no terminal cosplay, no decorative glow

Information style:

- operational facts first
- plain labels
- obvious empty states
- commands are copyable text, not hidden behind marketing copy

## Acceptance Criteria

- `make console` starts a local server without exposing secrets.
- `http://127.0.0.1:4317/` renders a dashboard from the real local ledger.
- `http://127.0.0.1:4317/healthz` returns JSON with `ok: true`.
- Clicking or posting to `/api/ledger-health` writes a redacted `ledger.health` event.
- The page shows conductor table counts and the latest redacted events.
- The page shows security posture: all conductor tables have RLS enabled and no direct `anon` / `authenticated` table grants.
- `cargo fmt --check`, `cargo test`, and `make ledger-health` pass.

## Distance To Full GUI

After v0, the GUI gap becomes feature depth, not foundation.

Remaining major layers:

- real work item intake from Linear/GitHub/Capy
- worker registry and heartbeat writes
- run/attempt lifecycle mutations
- artifact and PR link capture
- token/context usage charts
- memory snapshot references
- authenticated remote access
- richer motion and command-copy ergonomics

This v0 should make those layers visible and incrementally shippable instead of abstract.
