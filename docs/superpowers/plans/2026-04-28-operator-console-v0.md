# Operator Console v0 Implementation Plan

## Summary

Add a Rust-powered local GUI to Evensong-Conductor so the existing Supabase ledger can be inspected from a browser.

The implementation stays additive: `conductor-core` gains a small HTTP server, the Makefile gains `make console`, and docs explain the launch path. Upstream Symphony files remain untouched.

## Tasks

### 1. Console Server

- Add HTTP dependencies to `crates/conductor-core/Cargo.toml`.
- Add `crates/conductor-core/src/console.rs`.
- Serve:
  - `GET /`
  - `GET /healthz`
  - `POST /api/ledger-health`
- Query table counts, recent events, and security posture from the real database.
- Escape rendered strings before putting them into HTML.

### 2. CLI Integration

- Export `console` from `crates/conductor-core/src/lib.rs`.
- Add `serve-console --bind 127.0.0.1:4317` to `crates/conductor-core/src/main.rs`.
- Keep `ledger-health` unchanged.

### 3. Launch Path

- Add `make console`.
- Update first-launch output to point at the console URL.
- Update Evensong-Conductor docs and root README.

### 4. Verification

- Run `cargo fmt --check`.
- Run `cargo test`.
- Run `make ledger-health`.
- Start the console.
- Verify `/healthz` returns JSON success.
- Open the dashboard in a browser and confirm the first screen renders real ledger data.

## Rollback

The change is additive. Rollback is a single commit revert. The existing ledger schema and `ledger-health` CLI remain compatible.
