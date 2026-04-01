//! Ledger Processor Lambda for the Soroban block explorer.
//!
//! Processes LedgerCloseMeta payloads from S3 and persists structured data to PostgreSQL.

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    tracing::info!("indexer started");
}
