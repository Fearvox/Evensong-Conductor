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

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Evensong-Conductor Operator Console</title>
  <style>{}</style>
</head>
<body>
  <main>
    <header class="topbar">
      <div>
        <p class="eyebrow">Evensong-Conductor</p>
        <h1>Operator Console</h1>
      </div>
      <nav>
        <a href="/">Refresh</a>
        <a href="http://127.0.0.1:54323" rel="noreferrer">Supabase Studio</a>
      </nav>
    </header>

    <section class="hero">
      <div>
        <p class="section-label">Current state</p>
        <h2>Ledger online. GUI foundation live.</h2>
        <p class="lede">This console reads the local Supabase ledger directly. It shows redacted operational facts only: table counts, event history, and database guardrails.</p>
      </div>
      <form action="/api/ledger-health" method="post">
        <button type="submit">Write health event</button>
      </form>
    </section>

    <section class="metrics">
      {}
    </section>

    <section class="split">
      <article class="panel">
        <div class="panel-head">
          <p class="section-label">Security posture</p>
          <span>{} conductor tables</span>
        </div>
        {}
      </article>

      <article class="panel">
        <div class="panel-head">
          <p class="section-label">Launch commands</p>
          <span>No secrets rendered</span>
        </div>
        {}
      </article>
    </section>

    <section class="panel">
      <div class="panel-head">
        <p class="section-label">Recent events</p>
        <span>{event_count} total events</span>
      </div>
      {}
    </section>

    <footer>
      <span>Generated {}</span>
      <span>Local-only v0</span>
    </footer>
  </main>
</body>
</html>"#,
        base_css(),
        render_metrics(&snapshot.table_counts),
        snapshot.security.conductor_tables,
        render_security(&snapshot.security),
        render_commands(),
        render_events(&snapshot.recent_events),
        escape_html(&snapshot.generated_at)
    )
}

fn render_metrics(table_counts: &[TableCount]) -> String {
    table_counts
        .iter()
        .map(|count| {
            format!(
                r#"<article class="metric"><span>{}</span><strong>{}</strong><small>{}</small></article>"#,
                escape_html(count.label),
                count.count,
                escape_html(count.table)
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_security(security: &SecurityPosture) -> String {
    let rls_status = if security.rls_disabled == 0 {
        "All conductor tables have RLS enabled."
    } else {
        "Some conductor tables have RLS disabled."
    };

    let grant_status = if security.public_role_grants == 0 {
        "No direct anon/authenticated table grants found."
    } else {
        "Direct anon/authenticated table grants found."
    };

    format!(
        r#"<div class="status-list">
  <div class="status-row"><span>{}</span><strong>{}</strong></div>
  <div class="status-row"><span>{}</span><strong>{}</strong></div>
</div>"#,
        escape_html(rls_status),
        security.rls_disabled,
        escape_html(grant_status),
        security.public_role_grants
    )
}

fn render_commands() -> String {
    let commands = [
        ("Start console", "make console"),
        ("Write ledger event", "make ledger-health"),
        ("First launch", "make first-launch"),
        ("Run tests", "cargo test"),
    ];

    let rows = commands
        .iter()
        .map(|(label, command)| {
            format!(
                r#"<div class="command-row"><span>{}</span><code>{}</code></div>"#,
                escape_html(label),
                escape_html(command)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(r#"<div class="command-list">{rows}</div>"#)
}

fn render_events(events: &[EventRow]) -> String {
    if events.is_empty() {
        return r#"<div class="empty">No events yet. Run <code>make ledger-health</code> or press the health button.</div>"#
            .to_string();
    }

    let rows = events
        .iter()
        .map(|event| {
            format!(
                r#"<tr>
  <td>#{}</td>
  <td>{}</td>
  <td><span class="pill">{}</span></td>
  <td>{}</td>
  <td><code>{}</code></td>
  <td>{}</td>
</tr>"#,
                event.id,
                escape_html(&event.event_type),
                escape_html(&event.severity),
                escape_html(&event.message),
                escape_html(&event.payload_redacted.to_string()),
                escape_html(&event.created_at.to_rfc3339())
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<div class="table-wrap"><table>
  <thead>
    <tr>
      <th>ID</th>
      <th>Type</th>
      <th>Severity</th>
      <th>Message</th>
      <th>Payload</th>
      <th>Created</th>
    </tr>
  </thead>
  <tbody>{rows}</tbody>
</table></div>"#
    )
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
    <section class="hero">
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
  --bg: #f7f5ef;
  --text: #1b1a17;
  --muted: #706b62;
  --panel: rgba(255, 255, 255, 0.82);
  --panel-strong: #ffffff;
  --border: rgba(38, 37, 31, 0.13);
  --accent: #14593c;
  --accent-soft: #e7f1ea;
  --shadow: 0 18px 55px rgba(26, 24, 20, 0.08);
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "Helvetica Neue", Arial, sans-serif;
  letter-spacing: 0;
}

main {
  width: min(1180px, calc(100vw - 32px));
  margin: 0 auto;
  padding: 34px 0 28px;
}

a {
  color: inherit;
  text-decoration: none;
}

.topbar,
.hero,
.panel,
.metric {
  border: 1px solid var(--border);
  background: var(--panel);
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
  border-radius: 8px;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 18px 20px;
}

.topbar h1,
.hero h2 {
  margin: 0;
  font-weight: 560;
  line-height: 0.96;
}

.topbar h1 {
  font-size: clamp(28px, 4vw, 56px);
}

.topbar nav {
  display: flex;
  gap: 10px;
  color: var(--muted);
  font-size: 14px;
}

.topbar nav a,
button {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--panel-strong);
}

.hero {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 24px;
  align-items: end;
  margin-top: 14px;
  padding: 34px;
  box-shadow: var(--shadow);
}

.hero h2 {
  max-width: 800px;
  font-size: clamp(42px, 8vw, 92px);
}

.lede {
  max-width: 650px;
  color: var(--muted);
  font-size: 17px;
  line-height: 1.65;
}

button {
  cursor: pointer;
  color: #fff;
  background: var(--accent);
  font: inherit;
  font-weight: 600;
}

.eyebrow,
.section-label {
  margin: 0 0 10px;
  color: var(--accent);
  font-family: "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  font-size: 12px;
  text-transform: uppercase;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 14px;
}

.metric {
  min-height: 132px;
  padding: 18px;
}

.metric span,
.metric small,
.panel-head span {
  color: var(--muted);
}

.metric strong {
  display: block;
  margin: 20px 0 4px;
  font-size: 42px;
  font-weight: 560;
}

.split {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 14px;
  margin-top: 14px;
}

.panel {
  margin-top: 14px;
  padding: 22px;
}

.split .panel {
  margin-top: 0;
}

.panel-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
}

.status-list,
.command-list {
  display: grid;
  gap: 10px;
}

.status-row,
.command-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 13px 0;
  border-top: 1px solid var(--border);
}

code {
  color: var(--accent);
  font-family: "SF Mono", ui-monospace, Menlo, Consolas, monospace;
  font-size: 13px;
  white-space: pre-wrap;
  word-break: break-word;
}

.table-wrap {
  overflow-x: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
  min-width: 840px;
}

th,
td {
  padding: 13px 8px;
  border-top: 1px solid var(--border);
  text-align: left;
  vertical-align: top;
  font-size: 14px;
}

th {
  color: var(--muted);
  font-weight: 520;
}

.pill {
  display: inline-block;
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 4px 8px;
  color: var(--accent);
  background: var(--accent-soft);
}

.empty {
  padding: 30px;
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--muted);
}

footer {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 4px 0;
  color: var(--muted);
  font-size: 12px;
}

@media (max-width: 840px) {
  main {
    width: min(100vw - 20px, 1180px);
    padding-top: 10px;
  }

  .topbar,
  .hero,
  .split,
  footer {
    display: block;
  }

  .topbar nav {
    margin-top: 14px;
  }

  .hero {
    padding: 24px;
  }

  .hero form {
    margin-top: 22px;
  }

  .metrics {
    grid-template-columns: 1fr;
  }

  .split {
    display: grid;
    grid-template-columns: 1fr;
  }
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #11110f;
    --text: #f4f1e8;
    --muted: #aaa49a;
    --panel: rgba(27, 26, 23, 0.82);
    --panel-strong: #20201d;
    --border: rgba(244, 241, 232, 0.14);
    --accent: #b7f0c6;
    --accent-soft: rgba(183, 240, 198, 0.12);
    --shadow: 0 18px 55px rgba(0, 0, 0, 0.26);
  }

  button {
    color: #10120f;
  }
}
"#
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
    use super::escape_html;

    #[test]
    fn escapes_html_sensitive_characters() {
        assert_eq!(
            escape_html("<tag a=\"1\">Tom & 'Jerry'</tag>"),
            "&lt;tag a=&quot;1&quot;&gt;Tom &amp; &#39;Jerry&#39;&lt;/tag&gt;"
        );
    }
}
