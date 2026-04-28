# Conductor Room GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace the simple ledger dashboard with the accepted Conductor Room primary screen.

**Architecture:** Keep the Rust `conductor-core` HTTP server. Extend `console.rs` with a product-shell view model that combines live ledger/security facts with seed UI models for task queue, terminal blocks, CLI registry tabs, artifacts, and model budgets. Keep `/healthz` and the redacted health-event write path intact.

**Tech Stack:** Rust, axum, sqlx, server-rendered HTML/CSS, Supabase Postgres.

---

## File Structure

- Modify `.gitignore`: ignore `.superpowers/` visual companion state.
- Modify `crates/conductor-core/src/console.rs`: render Conductor Room HTML, seed UI models, and tests.
- Add `docs/superpowers/specs/2026-04-28-conductor-room-gui-design.md`: accepted design spec.
- Add `docs/superpowers/plans/2026-04-28-conductor-room-gui.md`: this plan.

## Task 1: Protect Design Scratch State

**Files:**
- Modify: `.gitignore`

- [x] **Step 1: Add visual companion state to `.gitignore`**

Add:

```gitignore
/.superpowers/
```

- [x] **Step 2: Verify git no longer tracks brainstorm state**

Run:

```bash
git status --short
```

Expected: no `?? .superpowers/` entry.

## Task 2: Add Product Shell View Model

**Files:**
- Modify: `crates/conductor-core/src/console.rs`

- [x] **Step 1: Add seed model structs**

Add structs for `TaskItem`, `TerminalBlock`, `ArtifactItem`, `BudgetItem`, and `CliTab` near the existing view structs. These types should hold static display values only and must not contain private paths or endpoints.

- [x] **Step 2: Add seed functions**

Add functions:

```rust
fn task_items() -> Vec<TaskItem>
fn terminal_blocks() -> Vec<TerminalBlock>
fn artifact_items() -> Vec<ArtifactItem>
fn budget_items() -> Vec<BudgetItem>
fn cli_tabs() -> Vec<CliTab>
```

Expected: each function returns the exact visible content described in the spec.

- [x] **Step 3: Keep live ledger data in `ConsoleSnapshot`**

Do not replace `ConsoleSnapshot`. Use it as the live data source for event count, recent events, and security posture.

## Task 3: Render Conductor Room Layout

**Files:**
- Modify: `crates/conductor-core/src/console.rs`

- [x] **Step 1: Replace `render_dashboard` structure**

Render the main layout:

```html
<main class="app-shell">
  <aside class="rail">...</aside>
  <section class="task-pane">...</section>
  <section class="run-room">...</section>
  <aside class="inspector">...</aside>
</main>
```

- [x] **Step 2: Add top command bar**

Include the command bar inside `run-room` or as a full-width top row with live status values:

- `Local Supabase online`
- `{event_count} events`
- security status derived from `public_role_grants`

- [x] **Step 3: Add responsive collapse**

Add CSS media queries:

- desktop: rail + task pane + run room + inspector
- tablet: task pane and inspector stack below run room
- mobile: one column, rail becomes horizontal navigation

## Task 4: Render Panels and Blocks

**Files:**
- Modify: `crates/conductor-core/src/console.rs`

- [x] **Step 1: Add render helpers**

Add:

```rust
fn render_task_queue() -> String
fn render_cli_tabs() -> String
fn render_terminal_blocks() -> String
fn render_inspector(snapshot: &ConsoleSnapshot) -> String
fn render_budget_items() -> String
fn render_artifacts() -> String
```

- [x] **Step 2: Preserve escaping**

All dynamic strings go through `escape_html`.

- [x] **Step 3: Preserve live health action**

Keep a form posting to `/api/ledger-health`.

## Task 5: Replace CSS With Product UI System

**Files:**
- Modify: `crates/conductor-core/src/console.rs`

- [x] **Step 1: Replace `base_css`**

Use DASH tokens:

- cream background
- deep green rail
- thin borders
- warm white panels
- monospace terminal blocks
- restrained neon yellow accent

- [x] **Step 2: Add interaction states**

Add hover/focus/selected styles for:

- nav items
- task rows
- tabs
- buttons
- terminal blocks

- [x] **Step 3: Add reduced-motion guard**

Add:

```css
@media (prefers-reduced-motion: reduce) {
  * { transition: none !important; animation: none !important; }
}
```

## Task 6: Tests and Verification

**Files:**
- Modify: `crates/conductor-core/src/console.rs`

- [x] **Step 1: Add render smoke test**

Add a unit test that creates a synthetic `ConsoleSnapshot`, renders the dashboard, and asserts it includes:

- `Conductor Room`
- `EVENS-018 GUI Task Room`
- `Hermes`
- `Context window / budget`
- `No public grants`

- [x] **Step 2: Run Rust checks**

Run:

```bash
cargo fmt --check
cargo test
make ledger-health
```

Expected: all pass.

- [x] **Step 3: Run browser checks**

Restart the console and verify:

```bash
curl -fsS http://<local-console>/healthz
curl -fsS http://<local-console>/ | rg "Conductor Room|EVENS-018|Context window"
```

Expected: JSON health is ok and HTML contains the primary screen anchors.

- [x] **Step 4: Visual QA**

Use browser screenshots at desktop and mobile widths. Compare against the accepted concept image and write a short fidelity ledger before final handoff.
