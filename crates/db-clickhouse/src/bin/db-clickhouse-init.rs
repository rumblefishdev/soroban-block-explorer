//! Apply the ClickHouse pilot schema to a target instance.
//!
//! Idempotent — every statement is `CREATE … IF NOT EXISTS`, so running
//! twice is a no-op. Used by:
//!   - the `db-clickhouse-init` sidecar in `docker-compose.yml`
//!   - local dev: `cargo run -p db-clickhouse --bin db-clickhouse-init`
//!
//! Reads `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`,
//! `CLICKHOUSE_DATABASE` from the environment; sane defaults for local dev.

use db_clickhouse::{Config, apply_init_sql, client};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = Config::from_env();
    tracing::info!(
        url = %cfg.url,
        user = %cfg.user,
        database = %cfg.database,
        "applying ClickHouse pilot schema"
    );

    let client = client(&cfg);
    apply_init_sql(&client).await?;

    tracing::info!("schema applied successfully");
    Ok(())
}
