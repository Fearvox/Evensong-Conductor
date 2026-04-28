use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::ledger;

const CONSOLE_TIME_FORMAT: &str = "%b %d, %H:%M UTC";

const CONDUCTOR_TABLES: &[(&str, &str)] = &[
    ("conductor_projects", "Projects"),
    ("conductor_work_items", "Work items"),
    ("conductor_runs", "Runs"),
    ("conductor_run_attempts", "Attempts"),
    ("conductor_workers", "Workers"),
    ("conductor_worker_leases", "Leases"),
    ("conductor_events", "Events"),
    ("conductor_artifacts", "Artifacts"),
    ("conductor_model_usage", "Model usage"),
];

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[derive(Debug)]
struct ConsoleSnapshot {
    generated_at: String,
    table_counts: Vec<TableCount>,
    recent_events: Vec<EventRow>,
    security: SecurityPosture,
}

#[derive(Debug)]
struct TableCount {
    table: &'static str,
    label: &'static str,
    count: i64,
}

#[derive(Debug)]
struct TaskItem {
    section: &'static str,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    sources: &'static [&'static str],
    agent: &'static str,
    meta: &'static str,
    progress: u8,
    review_label: Option<&'static str>,
}

#[derive(Debug)]
struct CliTab {
    label: &'static str,
    active: bool,
}

#[derive(Debug)]
struct TerminalBlock {
    title: &'static str,
    status: &'static str,
    duration: &'static str,
    body: &'static [&'static str],
    active: bool,
}

#[derive(Debug)]
struct ArtifactItem {
    name: &'static str,
    size: &'static str,
    kind: &'static str,
}

#[derive(Debug)]
struct BudgetItem {
    label: &'static str,
    percent: u8,
    used: &'static str,
    total: &'static str,
}

#[derive(Debug)]
struct EventRow {
    id: i64,
    event_type: String,
    severity: String,
    message: String,
    payload_redacted: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct SecurityPosture {
    conductor_tables: i64,
    rls_disabled: i64,
    public_role_grants: i64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    event_count: i64,
    generated_at: String,
}

pub async fn serve(pool: PgPool, bind: SocketAddr) -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/healthz", get(healthz))
        .route("/api/ledger-health", post(write_ledger_health))
        .with_state(AppState { pool });

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("conductor console listening on http://{bind}");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    match load_snapshot(&state.pool).await {
        Ok(snapshot) => Html(render_dashboard(&snapshot)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(render_error_page(&error.to_string())),
        )
            .into_response(),
    }
}

async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    match event_count(&state.pool).await {
        Ok(event_count) => (
            StatusCode::OK,
            Json(HealthResponse {
                ok: true,
                event_count,
                generated_at: Utc::now().to_rfc3339(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": error.to_string(),
                "generated_at": Utc::now().to_rfc3339(),
            })),
        )
            .into_response(),
    }
}

async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn write_ledger_health(State(state): State<AppState>) -> impl IntoResponse {
    match ledger::write_health_event(&state.pool).await {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(render_error_page(&error.to_string())),
        )
            .into_response(),
    }
}

async fn load_snapshot(pool: &PgPool) -> Result<ConsoleSnapshot> {
    let mut table_counts = Vec::with_capacity(CONDUCTOR_TABLES.len());

    for (table, label) in CONDUCTOR_TABLES {
        let sql = format!("select count(*)::bigint as count from {table}");
        let count = sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool).await?;
        table_counts.push(TableCount {
            table,
            label,
            count,
        });
    }

    let recent_events = recent_events(pool).await?;
    let security = security_posture(pool).await?;

    Ok(ConsoleSnapshot {
        generated_at: Utc::now().to_rfc3339(),
        table_counts,
        recent_events,
        security,
    })
}

async fn recent_events(pool: &PgPool) -> Result<Vec<EventRow>> {
    let rows = sqlx::query(
        r#"
        select id, event_type, severity, message, payload_redacted, created_at
        from conductor_events
        order by id desc
        limit 25
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| EventRow {
            id: row.get("id"),
            event_type: row.get("event_type"),
            severity: row.get("severity"),
            message: row.get("message"),
            payload_redacted: row.get("payload_redacted"),
            created_at: row.get("created_at"),
        })
        .collect())
}

async fn security_posture(pool: &PgPool) -> Result<SecurityPosture> {
    let conductor_tables = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from pg_class c
        join pg_namespace n on n.oid = c.relnamespace
        where n.nspname = 'public'
          and c.relkind = 'r'
          and c.relname like 'conductor_%'
        "#,
    )
    .fetch_one(pool)
    .await?;

    let rls_disabled = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from pg_class c
        join pg_namespace n on n.oid = c.relnamespace
        where n.nspname = 'public'
          and c.relkind = 'r'
          and c.relname like 'conductor_%'
          and not c.relrowsecurity
        "#,
    )
    .fetch_one(pool)
    .await?;

    let public_role_grants = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from information_schema.role_table_grants
        where table_schema = 'public'
          and table_name like 'conductor_%'
          and grantee in ('anon', 'authenticated')
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(SecurityPosture {
        conductor_tables,
        rls_disabled,
        public_role_grants,
    })
}

async fn event_count(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>("select count(*)::bigint from conductor_events")
        .fetch_one(pool)
        .await?;

    Ok(count)
}

fn render_dashboard(snapshot: &ConsoleSnapshot) -> String {
    let event_count = snapshot
        .table_counts
        .iter()
        .find(|count| count.table == "conductor_events")
        .map(|count| count.count)
        .unwrap_or_default();
    let grant_status = if snapshot.security.public_role_grants == 0 {
        "No public grants"
    } else {
        "Public grants found"
    };
    let latest_event = snapshot
        .recent_events
        .first()
        .map(|event| format_console_time(&event.created_at))
        .unwrap_or_else(|| "No events yet".to_string());

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Evensong-Conductor Room</title>
  <style>{}</style>
</head>
<body>
  <main class="app-shell" aria-label="Evensong-Conductor Room">
    {}
    <section class="task-pane" aria-label="Task queue">
      <div class="pane-heading">
        <span>Task Queue</span>
        <strong>{}</strong>
      </div>
      {}
    </section>
    <section class="run-room" aria-label="Live run room">
      {}
      <section class="run-stage">
        <div class="run-title">
          <div>
            <p>Live Run</p>
            <h1>GUI Task Room</h1>
          </div>
          <div class="run-meta">
            <span>LIVE</span>
            <b>Started by Hermes</b>
          </div>
          <a class="details-link" href="/">Refresh</a>
        </div>
        {}
        <div class="terminal-switcher" aria-label="Terminal views">
          <button type="button" class="switch active">Terminal</button>
          <button type="button" class="switch">Logs</button>
          <button type="button" class="switch">Events</button>
          <button type="button" class="switch">Files</button>
          <button type="button" class="switch">Env</button>
          <button type="button" class="switch">Resources</button>
        </div>
        {}
        {}
      </section>
    </section>
    {}
  </main>
</body>
</html>"#,
        base_css(),
        render_rail(),
        escape_html(grant_status),
        render_task_queue(),
        render_command_bar(event_count, grant_status),
        render_cli_tabs(),
        render_terminal_blocks(),
        render_composer(),
        render_inspector(snapshot, event_count, grant_status, &latest_event)
    )
}

fn render_rail() -> String {
    let nav_items = [
        ("Queue", true),
        ("Agents", false),
        ("Runs", false),
        ("Memory", false),
        ("Settings", false),
    ];
    let nav = nav_items
        .iter()
        .map(|(label, active)| {
            format!(
                r#"<a class="rail-link{}" href="/"><span>{}</span></a>"#,
                if *active { " active" } else { "" },
                escape_html(label)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<aside class="rail" aria-label="Product navigation">
  <a class="rail-mark" href="/" aria-label="Evensong-Conductor home">EC</a>
  <nav>{nav}</nav>
  <div class="rail-foot">
    <span>Operator</span>
    <strong>v0.1</strong>
  </div>
</aside>"#
    )
}

fn render_command_bar(event_count: i64, grant_status: &str) -> String {
    format!(
        r#"<header class="command-bar">
  <a class="brand-lockup" href="/">
    <span>DASH</span>
    <strong>Evensong-Conductor</strong>
  </a>
  <label class="command-input">
    <span>⌘</span>
    <input aria-label="Command input" value="Ask Conductor or run command..." readonly>
  </label>
  <div class="status-chips" aria-label="Console status">
    <span class="status-chip good">Local Supabase online</span>
    <span class="status-chip">{} events</span>
    <span class="status-chip{}">{}</span>
  </div>
  <form action="/api/ledger-health" method="post">
    <button class="primary-action" type="submit">New run</button>
  </form>
</header>"#,
        event_count,
        if grant_status == "No public grants" {
            " good"
        } else {
            " warn"
        },
        escape_html(grant_status)
    )
}

fn render_task_queue() -> String {
    let tasks = task_items();
    ["Now running", "Ready", "Needs review"]
        .iter()
        .map(|section| {
            let rows = tasks
                .iter()
                .filter(|task| task.section == *section)
                .map(render_task_item)
                .collect::<Vec<_>>()
                .join("");

            format!(
                r#"<section class="task-section">
  <h2>{}</h2>
  <div class="task-list">{rows}</div>
</section>"#,
                escape_html(section)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_task_item(task: &TaskItem) -> String {
    let source_chips = task
        .sources
        .iter()
        .map(|source| format!(r#"<span>{}</span>"#, escape_html(source)))
        .collect::<Vec<_>>()
        .join("");
    let review = task
        .review_label
        .map(|label| format!(r#"<b>{}</b>"#, escape_html(label)))
        .unwrap_or_default();
    let progress = task.progress.min(100);

    format!(
        r#"<article class="task-card" aria-label="{} {}">
  <div class="task-card-top">
    <span>{}</span>
    {}
  </div>
  <h3>{}</h3>
  <p>{}</p>
  <div class="source-chips">{}</div>
  <div class="task-card-foot">
    <span>{}</span>
    <strong>{}</strong>
  </div>
  <div class="progress" aria-label="{} percent complete"><span style="width: {}%"></span></div>
</article>"#,
        escape_html(task.id),
        escape_html(task.title),
        escape_html(task.id),
        review,
        escape_html(task.title),
        escape_html(task.description),
        source_chips,
        escape_html(task.agent),
        escape_html(task.meta),
        progress,
        progress
    )
}

fn render_cli_tabs() -> String {
    let tabs = cli_tabs()
        .iter()
        .map(|tab| {
            format!(
                r#"<button type="button" class="cli-tab{}">{}</button>"#,
                if tab.active { " active" } else { "" },
                escape_html(tab.label)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<nav class="cli-tabs" aria-label="CLI workers">{tabs}</nav>"#)
}

fn render_terminal_blocks() -> String {
    let blocks = terminal_blocks()
        .iter()
        .map(|block| {
            let body = block
                .body
                .iter()
                .map(|line| format!(r#"<p><code>{}</code></p>"#, escape_html(line)))
                .collect::<Vec<_>>()
                .join("");
            format!(
                r#"<article class="terminal-block{}">
  <header>
    <span>{}</span>
    <div><b>{}</b><small>{}</small></div>
  </header>
  <div class="terminal-body">{body}</div>
</article>"#,
                if block.active { " active" } else { "" },
                escape_html(block.title),
                escape_html(block.status),
                escape_html(block.duration)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<div class="terminal-stack">{blocks}</div>"#)
}

fn render_composer() -> String {
    r#"<section class="composer" aria-label="Command composer">
  <div>
    <span>Target agent</span>
    <strong>Codex / gpt-5.5</strong>
  </div>
  <label>
    <span>Command</span>
    <input value="Implement task panel and CLI registry; keep evidence current." readonly>
  </label>
  <button type="button">Send</button>
  <footer>
    <span>cwd: repo root</span>
    <span>shell: zsh</span>
    <span>autoscroll: on</span>
  </footer>
</section>"#
        .to_string()
}

fn render_inspector(
    snapshot: &ConsoleSnapshot,
    event_count: i64,
    grant_status: &str,
    latest_event: &str,
) -> String {
    let rls_status = if snapshot.security.rls_disabled == 0 {
        "RLS enabled"
    } else {
        "RLS needs review"
    };
    let memory_snapshot = serde_json::to_string_pretty(&serde_json::json!({
        "run": "EVENS-018",
        "mode": "operator-visible",
        "ledger": "redacted",
        "next": "task-panel-cli-registry"
    }))
    .unwrap_or_else(|_| "{}".to_string());

    format!(
        r#"<aside class="inspector" aria-label="Evidence inspector">
  <section class="inspector-card evidence-card">
    <div class="card-head">
      <span>Evidence</span>
      <strong>REDACTED</strong>
    </div>
    <div class="evidence-grid">
      <div class="evidence-primary"><span>Ledger health</span><b>{} events</b><small>redacted local ledger</small></div>
      <div><span>RLS status</span><b>{}</b></div>
      <div><span>Last event</span><b>{}</b></div>
      <div><span>Public grants</span><b>{}</b></div>
      <div><span>Generated</span><b>{}</b></div>
    </div>
    {}
    <form action="/api/ledger-health" method="post">
      <button type="submit">Write health event</button>
    </form>
  </section>
  <section class="inspector-card">
    <div class="card-head">
      <span>Memory snapshot</span>
      <strong>safe</strong>
    </div>
    <pre>{}</pre>
  </section>
  <section class="inspector-card">
    <div class="card-head">
      <span>Artifacts</span>
      <strong>{}</strong>
    </div>
    {}
  </section>
  <section class="inspector-card pr-card">
    <div class="card-head">
      <span>Pull Request</span>
      <strong>draft</strong>
    </div>
    <p>branch: <code>codex/evensong-conductor-foundation</code></p>
    <p>CI: local checks required before merge.</p>
  </section>
  <section class="inspector-card next-card">
    <div class="card-head">
      <span>Next action</span>
      <strong>continue</strong>
    </div>
    <p>Wire CLI adapters after the room shell proves stable on desktop and mobile.</p>
    <form action="/api/ledger-health" method="post">
      <button type="submit">Continue run</button>
    </form>
  </section>
  <section class="inspector-card budget-card">
    <div class="card-head">
      <span>Context window / budget</span>
      <strong>live</strong>
    </div>
    {}
  </section>
  <section class="inspector-card table-card">
    <div class="card-head">
      <span>Ledger tables</span>
      <strong>{}</strong>
    </div>
    {}
  </section>
</aside>"#,
        event_count,
        escape_html(rls_status),
        escape_html(latest_event),
        escape_html(grant_status),
        escape_html(&format_generated_time(&snapshot.generated_at)),
        render_recent_event_summary(&snapshot.recent_events),
        escape_html(&memory_snapshot),
        artifact_items().len(),
        render_artifacts(),
        render_budget_items(),
        snapshot.security.conductor_tables,
        render_table_counts(&snapshot.table_counts)
    )
}

fn render_recent_event_summary(events: &[EventRow]) -> String {
    let Some(event) = events.first() else {
        return r#"<div class="recent-event-mini">No redacted events yet.</div>"#.to_string();
    };

    format!(
        r#"<div class="recent-event-mini">
  <span>Latest redacted event</span>
  <strong>#{}</strong>
  <p>{} / {} / {}</p>
  <code>{}</code>
  <small>{}</small>
</div>"#,
        event.id,
        escape_html(&event.event_type),
        escape_html(&event.severity),
        escape_html(&event.message),
        escape_html(&event.payload_redacted.to_string()),
        escape_html(&format_console_time(&event.created_at))
    )
}

fn render_artifacts() -> String {
    let artifacts = artifact_items()
        .iter()
        .map(|artifact| {
            format!(
                r#"<div class="artifact-row">
  <span>{}</span>
  <strong>{}</strong>
  <small>{}</small>
</div>"#,
                escape_html(artifact.name),
                escape_html(artifact.kind),
                escape_html(artifact.size)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<div class="artifact-list">{artifacts}</div>"#)
}

fn render_budget_items() -> String {
    let items = budget_items()
        .iter()
        .map(|item| {
            let percent = item.percent.min(100);
            format!(
                r#"<div class="budget-row">
  <div>
    <span>{}</span>
    <strong>{} / {}</strong>
  </div>
  <div class="budget-meter" aria-label="{} percent used"><span style="width: {}%"></span></div>
</div>"#,
                escape_html(item.label),
                escape_html(item.used),
                escape_html(item.total),
                percent,
                percent
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<div class="budget-list">{items}</div>"#)
}

fn render_table_counts(table_counts: &[TableCount]) -> String {
    table_counts
        .iter()
        .map(|count| {
            format!(
                r#"<div class="table-row"><span>{}</span><strong>{}</strong></div>"#,
                escape_html(count.label),
                count.count
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_error_page(error: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Evensong-Conductor Error</title>
  <style>{}</style>
</head>
<body>
  <main>
    <section class="error-shell">
      <div>
        <p class="section-label">Console error</p>
        <h1>Ledger read failed.</h1>
        <p class="lede">{}</p>
      </div>
    </section>
  </main>
</body>
</html>"#,
        base_css(),
        escape_html(error)
    )
}

fn base_css() -> &'static str {
    r#"
:root {
  color-scheme: light dark;
  --paper: #f4f0e6;
  --paper-soft: #faf8f1;
  --ink: #191815;
  --muted: #726d64;
  --muted-2: #9d9588;
  --green: #09251c;
  --green-2: #113f2e;
  --green-3: #1a6448;
  --line: rgba(25, 24, 21, 0.13);
  --panel: rgba(255, 253, 247, 0.84);
  --panel-solid: #fffdf7;
  --terminal: #081711;
  --accent: #ecec82;
  --warning: #b95d25;
  --ok: #1e6a4f;
  --shadow: 0 18px 55px rgba(31, 27, 19, 0.11);
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-height: 100vh;
  background:
    radial-gradient(circle at 48% 18%, rgba(255, 255, 255, 0.72), transparent 30rem),
    linear-gradient(90deg, rgba(17, 63, 46, 0.07), transparent 34%),
    var(--paper);
  color: var(--ink);
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "Helvetica Neue", Arial, sans-serif;
  letter-spacing: 0;
  overflow-x: hidden;
}

html {
  overflow-x: hidden;
}

a {
  color: inherit;
  text-decoration: none;
}

.app-shell {
  display: grid;
  grid-template-columns: 72px 320px minmax(520px, 1fr) 340px;
  min-height: 100vh;
  max-width: 100vw;
  overflow-x: hidden;
}

.rail {
  position: sticky;
  top: 0;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  padding: 14px 10px;
  background: var(--green);
  color: #e9e2d2;
}

.rail-mark {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  border: 1px solid rgba(236, 236, 130, 0.28);
  border-radius: 8px;
  color: var(--accent);
  font: 700 15px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.rail nav {
  display: grid;
  gap: 8px;
  margin-top: 28px;
}

.rail-link {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 42px;
  border-radius: 8px;
  color: rgba(233, 226, 210, 0.62);
  font-size: 11px;
  font-weight: 620;
  writing-mode: vertical-rl;
  transform: rotate(180deg);
  transition: background 160ms ease, color 160ms ease;
}

.rail-link.active,
.rail-link:hover {
  background: rgba(236, 236, 130, 0.13);
  color: var(--accent);
}

.rail-foot {
  margin-top: auto;
  display: grid;
  gap: 6px;
  color: rgba(233, 226, 210, 0.62);
  font-size: 11px;
  text-align: center;
}

.rail-foot strong {
  color: var(--accent);
  font-weight: 620;
}

.task-pane,
.run-room,
.inspector {
  min-width: 0;
  padding: 18px;
}

.task-pane {
  border-right: 1px solid var(--line);
  background: rgba(250, 248, 241, 0.58);
}

.pane-heading,
.card-head,
.run-title,
.task-card-top,
.task-card-foot,
.command-bar,
.terminal-block header,
.composer footer,
.artifact-row,
.table-row,
.budget-row > div:first-child {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.pane-heading {
  margin-bottom: 20px;
  color: var(--green-2);
  font: 650 13px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.pane-heading strong {
  color: var(--ok);
  font-size: 11px;
  text-transform: none;
}

.task-section {
  display: grid;
  gap: 10px;
  margin-bottom: 22px;
}

.task-section h2,
.card-head span,
.task-card-top span,
.artifact-row small,
.budget-row span,
.composer span,
.terminal-block small {
  margin: 0;
  color: var(--muted);
  font: 640 11px/1.3 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.task-list {
  display: grid;
  gap: 10px;
}

.task-card,
.inspector-card,
.command-bar,
.run-stage,
.composer {
  border: 1px solid var(--line);
  background: var(--panel);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border-radius: 8px;
  box-shadow: var(--shadow);
}

.task-card {
  padding: 14px;
  transition: transform 160ms ease, border-color 160ms ease, background 160ms ease;
}

.task-card:hover {
  transform: translateY(-1px);
  border-color: rgba(26, 100, 72, 0.28);
  background: rgba(255, 253, 247, 0.96);
}

.task-card h3 {
  margin: 12px 0 8px;
  font-size: 18px;
  line-height: 1.08;
  font-weight: 680;
}

.task-card p,
.inspector-card p,
.composer input,
.command-input input {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.45;
}

.task-card-top b {
  color: var(--warning);
  font: 700 11px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.source-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 12px 0;
}

.source-chips span,
.status-chip,
.cli-tab,
.switch {
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 6px 8px;
  background: rgba(255, 253, 247, 0.72);
  color: var(--muted);
  font: 640 11px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.task-card-foot strong {
  color: var(--green-2);
  font-size: 12px;
  font-weight: 690;
}

.progress,
.budget-meter {
  height: 5px;
  overflow: hidden;
  border-radius: 999px;
  background: rgba(9, 37, 28, 0.1);
}

.progress span,
.budget-meter span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--green-3), var(--accent));
}

.run-room {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 16px;
}

.command-bar {
  align-items: center;
  padding: 12px;
  box-shadow: none;
}

.brand-lockup {
  display: grid;
  gap: 3px;
  min-width: 176px;
}

.brand-lockup span {
  color: var(--green-3);
  font: 760 11px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.brand-lockup strong {
  font-size: 15px;
  line-height: 1;
}

.command-input {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 220px;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 0 12px;
  background: rgba(255, 253, 247, 0.82);
}

.command-input span {
  color: var(--green-3);
  font-weight: 720;
}

input,
button {
  border: 0;
  font: inherit;
}

input {
  width: 100%;
  min-width: 0;
  padding: 11px 0;
  background: transparent;
  color: var(--ink);
}

button {
  cursor: pointer;
}

.status-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  justify-content: flex-end;
}

.status-chip.good {
  color: var(--ok);
  background: rgba(30, 106, 79, 0.1);
}

.status-chip.warn {
  color: var(--warning);
  background: rgba(185, 93, 37, 0.1);
}

.primary-action,
.inspector-card button,
.composer button {
  border-radius: 8px;
  padding: 11px 13px;
  background: var(--green);
  color: var(--accent);
  font-size: 12px;
  font-weight: 740;
}

.run-stage {
  min-height: 0;
  padding: 18px;
}

.run-title {
  margin-bottom: 16px;
}

.run-title p {
  margin: 0 0 6px;
  color: var(--green-3);
  font: 640 12px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.run-title h1 {
  margin: 0;
  font-size: clamp(28px, 4vw, 38px);
  line-height: 1;
  font-weight: 690;
}

.run-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
  color: var(--muted);
  font: 640 12px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.run-meta span {
  border-radius: 6px;
  padding: 5px 7px;
  background: rgba(30, 106, 79, 0.12);
  color: var(--ok);
}

.run-meta b {
  font-weight: 640;
}

.details-link {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 10px 12px;
  color: var(--green-2);
  font-size: 13px;
  font-weight: 650;
}

.cli-tabs,
.terminal-switcher {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  padding-bottom: 8px;
}

.cli-tab,
.switch {
  white-space: nowrap;
}

.cli-tab.active,
.switch.active {
  border-color: rgba(9, 37, 28, 0.42);
  background: var(--green);
  color: var(--accent);
}

.terminal-stack {
  display: grid;
  gap: 10px;
  margin-top: 4px;
}

.terminal-block {
  overflow: hidden;
  border: 1px solid rgba(236, 236, 130, 0.12);
  border-radius: 8px;
  background: var(--terminal);
  color: #d8d2bd;
}

.terminal-block.active {
  border-color: rgba(236, 236, 130, 0.48);
  box-shadow: 0 20px 60px rgba(9, 37, 28, 0.25);
}

.terminal-block header {
  padding: 12px 14px;
  border-bottom: 1px solid rgba(236, 236, 130, 0.14);
}

.terminal-block header > span {
  color: var(--accent);
  font: 700 12px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
}

.terminal-block header div {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.terminal-block b {
  color: #f2edcf;
  font-size: 12px;
}

.terminal-body {
  display: grid;
  gap: 3px;
  padding: 14px;
}

.terminal-body p {
  margin: 0;
}

code,
pre {
  font-family: "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  letter-spacing: 0;
}

code {
  color: inherit;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
}

.composer {
  display: grid;
  grid-template-columns: 170px minmax(0, 1fr) auto;
  gap: 10px;
  margin-top: 12px;
  padding: 12px;
  box-shadow: none;
}

.composer > div,
.composer label {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.composer label {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px 10px;
  background: rgba(255, 253, 247, 0.72);
}

.composer strong {
  font-size: 13px;
}

.composer footer {
  grid-column: 1 / -1;
  justify-content: flex-start;
  color: var(--muted-2);
}

.eyebrow,
.section-label {
  margin: 0 0 10px;
  color: var(--green-3);
  font-family: "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  font-size: 12px;
  text-transform: uppercase;
}

.inspector {
  display: grid;
  align-content: start;
  gap: 12px;
  border-left: 1px solid var(--line);
  background: rgba(244, 240, 230, 0.7);
}

.inspector-card {
  padding: 14px;
  box-shadow: none;
}

.card-head {
  margin-bottom: 12px;
}

.card-head strong {
  color: var(--green-2);
  font: 700 11px/1 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.evidence-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  margin-bottom: 12px;
}

.evidence-grid div {
  display: grid;
  align-content: start;
  gap: 8px;
  min-height: 76px;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 10px;
  background: rgba(255, 253, 247, 0.66);
}

.evidence-grid .evidence-primary {
  grid-column: 1 / -1;
  min-height: 92px;
  background: rgba(30, 106, 79, 0.08);
}

.evidence-grid span,
.artifact-row span,
.table-row span {
  display: block;
  color: var(--muted);
  font-size: 12px;
}

.evidence-grid b {
  display: block;
  font-size: 13px;
  line-height: 1.25;
  overflow-wrap: anywhere;
}

.evidence-grid .evidence-primary b {
  font-size: 24px;
  color: var(--green-2);
}

.evidence-grid small {
  color: var(--muted);
  font: 640 11px/1.25 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.recent-event-mini {
  display: grid;
  gap: 6px;
  margin: 0 0 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 10px;
  background: rgba(9, 37, 28, 0.04);
}

.recent-event-mini span,
.recent-event-mini small {
  color: var(--muted);
  font: 640 11px/1.25 "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  text-transform: uppercase;
}

.recent-event-mini strong {
  color: var(--green-2);
  font-size: 13px;
}

.recent-event-mini p {
  color: var(--muted);
  font-size: 12px;
}

pre {
  max-height: 180px;
  overflow: auto;
  margin: 0;
  border-radius: 8px;
  padding: 12px;
  background: rgba(9, 37, 28, 0.06);
  color: var(--green-2);
  font-size: 11px;
  line-height: 1.48;
  white-space: pre-wrap;
}

.artifact-list,
.budget-list {
  display: grid;
  gap: 8px;
}

.artifact-row {
  align-items: baseline;
  border-top: 1px solid var(--line);
  padding-top: 9px;
}

.artifact-row strong {
  color: var(--green-2);
  font-size: 12px;
}

.budget-row {
  display: grid;
  gap: 8px;
  border-top: 1px solid var(--line);
  padding-top: 10px;
}

.budget-row strong {
  font-size: 12px;
  font-weight: 680;
}

.table-card {
  max-height: 310px;
  overflow: auto;
}

.table-row {
  border-top: 1px solid var(--line);
  padding: 8px 0;
}

.table-row span {
  margin-bottom: 0;
  font-size: 11px;
}

.error-shell {
  width: min(760px, calc(100vw - 32px));
  margin: 64px auto;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 28px;
  background: var(--panel);
}

.error-shell h1 {
  margin: 0 0 12px;
  font-size: 44px;
  line-height: 0.98;
}

@media (max-width: 1280px) {
  .app-shell {
    grid-template-columns: 64px 292px minmax(480px, 1fr);
  }

  .inspector {
    grid-column: 2 / -1;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-left: 0;
    border-top: 1px solid var(--line);
  }
}

@media (max-width: 920px) {
  html,
  body {
    max-width: 100vw;
    overflow-x: hidden;
  }

  .app-shell {
    display: block;
    width: 100vw;
    max-width: 100vw;
  }

  .rail {
    position: static;
    min-height: auto;
    flex-direction: row;
    align-items: center;
    gap: 10px;
    padding: 10px;
  }

  .rail nav {
    display: flex;
    gap: 6px;
    margin: 0;
    overflow-x: auto;
  }

  .rail-link {
    min-width: auto;
    min-height: 34px;
    padding: 0 10px;
    writing-mode: horizontal-tb;
    transform: none;
  }

  .rail-foot {
    display: none;
  }

  .task-pane,
  .run-room,
  .inspector {
    width: 100vw;
    max-width: 100vw;
    padding: 12px;
    border: 0;
    overflow-x: hidden;
  }

  .task-card,
  .command-bar,
  .run-stage,
  .inspector-card,
  .composer {
    width: 100%;
    max-width: calc(100vw - 24px);
  }

  .command-bar,
  .composer {
    display: grid;
    grid-template-columns: 1fr;
  }

  .brand-lockup,
  .command-input,
  .status-chips,
  .command-bar form {
    min-width: 0;
    width: 100%;
  }

  .status-chips {
    justify-content: flex-start;
  }

  .run-title {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
    align-items: flex-start;
  }

  .run-meta {
    margin-left: 0;
  }

  .details-link {
    justify-self: start;
  }

  .cli-tabs,
  .terminal-switcher {
    flex-wrap: wrap;
    overflow-x: visible;
  }

  .inspector {
    grid-template-columns: 1fr;
  }

  .evidence-grid {
    grid-template-columns: 1fr;
  }
}

@media (prefers-color-scheme: dark) {
  :root {
    --paper: #11110f;
    --paper-soft: #151511;
    --ink: #f4f1e8;
    --muted: #aaa49a;
    --muted-2: #817a6e;
    --green: #06170f;
    --green-2: #b7f0c6;
    --green-3: #7bd79f;
    --line: rgba(244, 241, 232, 0.14);
    --panel: rgba(27, 26, 23, 0.84);
    --panel-solid: #20201d;
    --terminal: #07110d;
    --accent: #e8f080;
    --ok: #92ddb0;
    --shadow: 0 18px 55px rgba(0, 0, 0, 0.26);
  }

  body {
    background:
      radial-gradient(circle at 48% 18%, rgba(123, 215, 159, 0.08), transparent 30rem),
      linear-gradient(90deg, rgba(123, 215, 159, 0.05), transparent 34%),
      var(--paper);
  }

  input {
    color: var(--ink);
  }
}

@media (prefers-reduced-motion: reduce) {
  * {
    animation: none !important;
    transition: none !important;
  }
}
"#
}

fn task_items() -> Vec<TaskItem> {
    vec![
        TaskItem {
            section: "Now running",
            id: "EVENS-018",
            title: "GUI Task Room",
            description: "Build the first operator surface where queue, terminals, and evidence are visible together.",
            sources: &["GSD", "Codex", "Design Infra"],
            agent: "Codex",
            meta: "active",
            progress: 74,
            review_label: None,
        },
        TaskItem {
            section: "Ready",
            id: "EVENS-017",
            title: "Multica Adapter",
            description: "Normalize local AI CLIs behind one task contract without hiding their native strengths.",
            sources: &["Multica", "Hermes", "MiMo"],
            agent: "MiMo",
            meta: "queued",
            progress: 18,
            review_label: None,
        },
        TaskItem {
            section: "Ready",
            id: "EVENS-016",
            title: "Warp Terminal Layer",
            description: "Turn terminal sessions into inspectable blocks instead of forcing GUI and TUI context switching.",
            sources: &["Warp", "tmux", "PTY"],
            agent: "Hermes",
            meta: "designing",
            progress: 31,
            review_label: None,
        },
        TaskItem {
            section: "Ready",
            id: "EVENS-015",
            title: "Evidence Pipeline",
            description: "Keep screenshots, logs, PR notes, and benchmark claims attached to the run that produced them.",
            sources: &["Supabase", "GitHub", "Vercel"],
            agent: "OpenClaw",
            meta: "mapped",
            progress: 46,
            review_label: None,
        },
        TaskItem {
            section: "Needs review",
            id: "EVENS-014",
            title: "Ledger Health Check",
            description: "Verify redacted events, RLS posture, and table counts before more adapters write into the ledger.",
            sources: &["Postgres", "RLS"],
            agent: "Codex",
            meta: "review",
            progress: 89,
            review_label: Some("audit"),
        },
        TaskItem {
            section: "Needs review",
            id: "EVENS-013",
            title: "RLS Policy Audit",
            description: "Confirm public roles cannot read operator material before exposing richer research memory surfaces.",
            sources: &["Supabase", "Security"],
            agent: "Codex",
            meta: "blocked",
            progress: 62,
            review_label: Some("security"),
        },
    ]
}

fn cli_tabs() -> Vec<CliTab> {
    vec![
        CliTab {
            label: "Codex",
            active: true,
        },
        CliTab {
            label: "Hermes",
            active: false,
        },
        CliTab {
            label: "MiMo",
            active: false,
        },
        CliTab {
            label: "Claude Code",
            active: false,
        },
        CliTab {
            label: "OpenClaw",
            active: false,
        },
        CliTab {
            label: "Gemini",
            active: false,
        },
    ]
}

fn terminal_blocks() -> Vec<TerminalBlock> {
    vec![
        TerminalBlock {
            title: "cargo test --workspace --locked",
            status: "complete",
            duration: "18s",
            body: &[
                "$ cargo test --workspace --locked",
                "conductor-core tests: pass",
                "ledger migrations: verified",
            ],
            active: false,
        },
        TerminalBlock {
            title: "make console",
            status: "complete",
            duration: "4s",
            body: &[
                "$ make console",
                "serve-console listener ready",
                "health route responding",
            ],
            active: false,
        },
        TerminalBlock {
            title: "Implement task panel and CLI registry",
            status: "running",
            duration: "now",
            body: &[
                "$ codex task EVENS-018 --focus gui-room",
                "render queue, run room, inspector",
                "preserve redacted ledger facts",
            ],
            active: true,
        },
        TerminalBlock {
            title: "tmux attach",
            status: "queued",
            duration: "next",
            body: &[
                "$ tmux attach -t evensong-conductor-console",
                "watch console logs without leaving the room",
            ],
            active: false,
        },
        TerminalBlock {
            title: "git status",
            status: "queued",
            duration: "final",
            body: &[
                "$ git status -sb",
                "stage only product shell changes",
                "push branch when checks pass",
            ],
            active: false,
        },
    ]
}

fn artifact_items() -> Vec<ArtifactItem> {
    vec![
        ArtifactItem {
            name: "conductor-room-desktop.png",
            size: "visual QA",
            kind: "screenshot",
        },
        ArtifactItem {
            name: "console-build.log",
            size: "local only",
            kind: "log",
        },
        ArtifactItem {
            name: "task-panel.diff",
            size: "reviewable",
            kind: "diff",
        },
    ]
}

fn budget_items() -> Vec<BudgetItem> {
    vec![
        BudgetItem {
            label: "GPT-5.5",
            percent: 15,
            used: "152K",
            total: "1M",
        },
        BudgetItem {
            label: "MiMo V2.5",
            percent: 4,
            used: "40K",
            total: "1M",
        },
        BudgetItem {
            label: "DeepSeek Judge",
            percent: 6,
            used: "$0.60",
            total: "$10",
        },
        BudgetItem {
            label: "Local Hermes",
            percent: 28,
            used: "280K",
            total: "1M",
        },
    ]
}

fn format_console_time(value: &DateTime<Utc>) -> String {
    value.format(CONSOLE_TIME_FORMAT).to_string()
}

fn format_generated_time(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|time| {
            time.with_timezone(&Utc)
                .format(CONSOLE_TIME_FORMAT)
                .to_string()
        })
        .unwrap_or_else(|_| "Just now".to_string())
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }

    escaped
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        ConsoleSnapshot, EventRow, SecurityPosture, TableCount, escape_html, render_dashboard,
    };

    #[test]
    fn escapes_html_sensitive_characters() {
        assert_eq!(
            escape_html("<tag a=\"1\">Tom & 'Jerry'</tag>"),
            "&lt;tag a=&quot;1&quot;&gt;Tom &amp; &#39;Jerry&#39;&lt;/tag&gt;"
        );
    }

    #[test]
    fn renders_conductor_room_primary_screen() {
        let snapshot = ConsoleSnapshot {
            generated_at: "2026-04-28T12:00:00Z".to_string(),
            table_counts: vec![
                TableCount {
                    table: "conductor_events",
                    label: "Events",
                    count: 4,
                },
                TableCount {
                    table: "conductor_runs",
                    label: "Runs",
                    count: 2,
                },
            ],
            recent_events: vec![EventRow {
                id: 1,
                event_type: "health".to_string(),
                severity: "info".to_string(),
                message: "ledger healthy".to_string(),
                payload_redacted: serde_json::json!({"status": "redacted"}),
                created_at: Utc::now(),
            }],
            security: SecurityPosture {
                conductor_tables: 9,
                rls_disabled: 0,
                public_role_grants: 0,
            },
        };

        let html = render_dashboard(&snapshot);

        assert!(html.contains("Conductor Room"));
        assert!(html.contains("EVENS-018 GUI Task Room"));
        assert!(html.contains("Hermes"));
        assert!(html.contains("Context window / budget"));
        assert!(html.contains("No public grants"));
    }
}
