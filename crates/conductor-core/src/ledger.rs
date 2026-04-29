use anyhow::Result;
use serde_json::json;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

use crate::config::ConductorConfig;

pub async fn connect(config: &ConductorConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    Ok(pool)
}

pub async fn write_health_event(pool: &PgPool) -> Result<i64> {
    let row = sqlx::query(
        r#"
        insert into conductor_events (event_type, severity, message, payload_redacted)
        values ($1, $2, $3, $4)
        returning id
        "#,
    )
    .bind("ledger.health")
    .bind("info")
    .bind("conductor ledger health check")
    .bind(json!({"source": "conductor-core"}))
    .fetch_one(pool)
    .await?;

    Ok(row.get("id"))
}
