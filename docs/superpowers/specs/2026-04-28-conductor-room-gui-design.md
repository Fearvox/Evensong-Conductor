# Conductor Room GUI Design

Date: 2026-04-28
Status: approved for implementation
Accepted concept: operator-side Conductor Room concept image
Brand source: operator-side DASH brand folder

## Product Goal

Turn the current ledger dashboard into the first real Evensong-Conductor Room: a polished operator workspace where task queue, live agent terminal, and evidence/memory inspector are visible at once.

This is not the final adapter implementation. It is the product shell that future Multica-style task orchestration, Warp-style terminal blocks, and CLI worker adapters will plug into.

## Concept Decision

The accepted direction is **C. Conductor Room**:

- left: task queue, agent/source assignment, product navigation
- center: live run room with CLI tabs and structured terminal blocks
- right: evidence, memory, artifacts, PR state, next action, model budget

The UI should feel like an operations room rather than a dashboard. The operator should not need to bounce between GUI, TUI, tmux, browser tabs, and markdown handoffs to understand what is happening.

## Visual System

Use DASH brand primitives:

- background: cream paper
- primary dark: deep green
- accents: mid green and restrained neon yellow
- surfaces: white/cream with thin borders and restrained shadows
- typography: system/SF/Plus-Jakarta-like UI; monospace for commands and data
- radius: 8px or less
- no grid background
- no decorative glow, gradient blobs, or terminal cosplay

The current favicon can stay as-is for this phase. A dedicated Conductor brand/favicon pass belongs after v0.0.1.

## Data Boundary

Keep live database facts:

- conductor table counts
- recent redacted events
- RLS enabled count
- direct `anon` / `authenticated` grant count
- `/healthz` event count
- health-event write action

Use seed UI models for product shell entities that do not yet have adapters:

- task queue rows
- CLI tabs
- terminal blocks
- evidence artifact list
- model/context budget meters
- PR and next-action preview

Seed models must be clearly operational and replaceable. They must not expose private endpoints, API keys, raw prompts, local absolute paths, or remote IPs.

## Primary Screen Inventory

### Top Command Bar

- product label: `DASH / Evensong-Conductor`
- search/command input: `Ask Conductor or run command...`
- status chips:
  - `Local Supabase online`
  - `{event_count} events`
  - `No public grants` or `Public grants found`
- primary action: `New run`

### Left Task Queue

Sections:

- `Now running`
- `Ready`
- `Needs review`

Rows:

- `EVENS-018 GUI Task Room`
- `EVENS-017 Multica Adapter`
- `EVENS-016 Warp Terminal Layer`
- `EVENS-015 Evidence Pipeline`
- `EVENS-014 Ledger Health Check`
- `EVENS-013 RLS Policy Audit`

Each row shows source chips, assigned agent, short description, and status/progress.

### Center Live Run Room

Header:

- `Live Run: GUI Task Room`
- selected worker status
- run details action

CLI tabs:

- Codex
- Hermes
- MiMo
- Claude Code
- OpenClaw
- Gemini

Terminal sub-tabs:

- Terminal
- Logs
- Events
- Files
- Env
- Resources

Terminal blocks:

- passed test block
- `make console` build block
- active implementation block
- queued tmux attach block
- queued git status block

Composer:

- target agent label
- natural language command input
- model selector
- send button
- working directory/shell/autoscroll status bar

### Right Inspector

Cards:

- Evidence: ledger health, RLS status, latest event source, event count
- Memory snapshot: redacted JSON-like snapshot
- Artifacts: screenshot, build log, diff
- Pull Request: branch, PR number placeholder, CI state
- Next action: concise instruction and continue button
- Context window / budget: GPT-5.5, MiMo V2.5, DeepSeek, local Hermes

## Interaction Requirements

- `Write health event` still writes a redacted ledger event.
- `Refresh` reloads the page.
- Copyable command-looking text must remain selectable.
- Dashboard controls may be visual-only for this phase if the underlying adapter is not implemented, but they should look and behave like stable UI rather than placeholders.
- The page must remain readable on a laptop viewport and collapse to a single-column mobile layout without overflow.

## Acceptance Criteria

- `/` renders the Conductor Room primary screen instead of the old ledger dashboard.
- `/healthz` remains unchanged and returns `ok: true`.
- The page uses live ledger event count and live security posture.
- The page includes all primary screen inventory sections above.
- No private URLs, API keys, local absolute paths, or raw terminal logs render in HTML.
- `cargo fmt --check` passes.
- `cargo test` passes.
- Browser verification checks desktop and mobile widths.
- Final review compares the accepted concept image with the browser screenshot.
