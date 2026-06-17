//! Shared ClickHouse staging+`EXCHANGE TABLES` helpers for the post-merge
//! maintenance subcommands (`repair-tier1`, `asset-aggregates`,
//! `contract-type-rebuild`).
//!
//! All three follow the same "build a staging table → atomically swap it in"
//! pattern (ClickHouse has no cheap in-place UPDATE; a re-INSERT can't bump the
//! RMT version column, so a whole-table rebuild + swap is the sanctioned way).
//! These primitives were duplicated per subcommand — consolidated here.

use clickhouse::Client as ClickhouseClient;
use tracing::{debug, info};

use crate::error::BackfillError;

/// `DROP TABLE IF EXISTS`.
pub(crate) async fn drop_if_exists(
    client: &ClickhouseClient,
    table: &str,
) -> Result<(), BackfillError> {
    client
        .query(&format!("DROP TABLE IF EXISTS {table}"))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

/// `CREATE TABLE <staging> AS <source>` — clones schema **and engine** (so the
/// staging table shares the source's `ReplacingMergeTree(version)` semantics
/// and `EXCHANGE` is a true atomic swap, not a schema rewrite). No data copied.
pub(crate) async fn create_staging_like(
    client: &ClickhouseClient,
    source: &str,
    staging: &str,
) -> Result<(), BackfillError> {
    client
        .query(&format!("CREATE TABLE {staging} AS {source}"))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    debug!(source, staging, "ch_staging: staging table created");
    Ok(())
}

/// `SELECT count() FROM <table>`.
pub(crate) async fn staging_row_count(
    client: &ClickhouseClient,
    table: &str,
) -> Result<u64, BackfillError> {
    let n: u64 = client
        .query(&format!("SELECT count() FROM {table}"))
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

/// Atomic `EXCHANGE TABLES <live> AND <staging>`.
pub(crate) async fn exchange_tables(
    client: &ClickhouseClient,
    live: &str,
    staging: &str,
) -> Result<(), BackfillError> {
    client
        .query(&format!("EXCHANGE TABLES {live} AND {staging}"))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    info!(live, staging, "ch_staging: tables exchanged");
    Ok(())
}

/// Finish a staging rebuild: on `dry_run` just drop the staging table (live
/// untouched); otherwise `EXCHANGE` it in, then drop the now-stale old data
/// that lands in the staging name after the swap.
pub(crate) async fn finalize(
    client: &ClickhouseClient,
    live: &str,
    staging: &str,
    dry_run: bool,
) -> Result<(), BackfillError> {
    if dry_run {
        debug!(live, staging, "ch_staging: dry-run, dropping staging");
        return drop_if_exists(client, staging).await;
    }
    exchange_tables(client, live, staging).await?;
    drop_if_exists(client, staging).await
}
