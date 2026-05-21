//! CH port of the PG `recompute_asset_aggregates` write-path helper
//! (task 0228 Phase 5 / task 0194 CH continuation).
//!
//! ## Why this exists
//!
//! On Postgres the indexer recomputes `assets.{holder_count,
//! total_supply}` for every affected `(asset_code, issuer_id)` pair
//! inline on each per-ledger commit
//! ([crates/indexer/src/handler/persist/write.rs:2659]). On ClickHouse
//! the per-ledger path stages writes batched across a partition and
//! cannot run that style of "join-against-account_balances_current and
//! UPDATE" recompute economically — every partition would do a full
//! table scan of `account_balances_current`. Instead, this module runs
//! the equivalent as a single post-merge pass: build a staging
//! `assets` table whose `(holder_count, total_supply)` reflects the
//! current state of `account_balances_current`, then EXCHANGE TABLES
//! to swap.
//!
//! ## Aggregation scope (PG parity)
//!
//! Same scope as the PG MVP: aggregate over trustlines only — classic
//! credit (`asset_type = 1`) and SAC-wrapped classic
//! (`asset_type = 2`). Native XLM, Soroban-native, claimable
//! balances, LP reserves, and per-SAC contract holdings are out of
//! scope (matching PG; the gap is captured under "Future Work" in
//! task 0194).
//!
//! Rows with `asset_type IN (0, 3)` (native + soroban-native) keep
//! their existing `total_supply` / `holder_count` values — the
//! `if(asset_type IN (1, 2), …)` projection short-circuits to the
//! original column.
//!
//! ## Aggregate shape
//!
//! `account_balances_current` has no `contract_id` column — the PK is
//! `(account_id, asset_type, asset_code, issuer_id)`. We therefore
//! aggregate by `(asset_code, issuer_id)` and the LEFT JOIN against
//! `assets` matches every row that shares those keys regardless of
//! `(asset_type, contract_id)`. This mirrors the PG path: an asset's
//! classic-credit row and its SAC-wrapped row both get the same
//! `total_supply` / `holder_count` because they share the same
//! underlying trustline balances.
//!
//! ## Reads use FINAL
//!
//! `account_balances_current FINAL` and `assets FINAL` — one-shot,
//! correctness-critical aggregate. Outside this pass, the
//! no-`FINAL`-at-query-time invariant (ADR 0044) holds.

use clickhouse::Client as ClickhouseClient;
use tracing::{debug, info};

use crate::error::BackfillError;
use crate::sink::Sink;

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetAggregatesStats {
    /// Number of rows in the rebuilt staging table — should match
    /// `count() FROM assets FINAL` modulo race with concurrent ingest
    /// (none expected at the time this runs, post-ATTACH).
    pub assets_rows: u64,
    pub dry_run: bool,
}

pub async fn execute(sink: &Sink, dry_run: bool) -> Result<AssetAggregatesStats, BackfillError> {
    let Sink::Clickhouse(client) = sink else {
        info!("asset_aggregates: skipped (PG target — handled inline per-ledger)");
        return Ok(AssetAggregatesStats::default());
    };

    let staging = "assets_staging_recompute";
    drop_if_exists(client, staging).await?;
    create_staging_like(client, "assets", staging).await?;

    // The IF guard around `agg.holder_count` / `agg.total_supply`
    // matches the PG `WHERE asset_type IN (1, 2)` clause: rows outside
    // that filter pass through with their existing values.
    //
    // `ifNull(agg.holder_count, 0)` / `ifNull(agg.total_supply, 0)`
    // collapse the LEFT JOIN miss to a literal 0 — the asset has no
    // trustlines (every position closed). PG path uses
    // `COALESCE(sub.cnt, 0)` / `COALESCE(sub.sum, 0)` for the same
    // reason.
    //
    // `assets.total_supply` is `Nullable(Decimal128(7))`. The
    // aggregate `sum(balance)` over `Decimal128(7)` returns
    // `Decimal128(7)` already; CAST is defensive against CH version
    // drift where `sum()` over Decimal returns Decimal with a
    // promoted scale.
    let insert_sql = format!(
        "INSERT INTO {staging} (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url)
         SELECT
             a.asset_type,
             a.asset_code,
             a.issuer_id,
             a.contract_id,
             a.name,
             if(a.asset_type IN (1, 2),
                CAST(ifNull(agg.total_supply, toDecimal128(0, 7)) AS Decimal128(7)),
                a.total_supply) AS total_supply,
             if(a.asset_type IN (1, 2),
                CAST(ifNull(agg.holder_count, 0) AS Int32),
                a.holder_count) AS holder_count,
             a.icon_url
           FROM assets AS a FINAL
           LEFT JOIN (
             SELECT
                 asset_code,
                 issuer_id,
                 countIf(balance > 0) AS holder_count,
                 sum(balance) AS total_supply
               FROM account_balances_current FINAL
              WHERE asset_type IN (1, 2)
              GROUP BY asset_code, issuer_id
           ) AS agg ON agg.asset_code = a.asset_code AND agg.issuer_id = a.issuer_id"
    );
    client
        .query(&insert_sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;

    let rows = staging_row_count(client, staging).await?;
    info!(staging, rows, "asset_aggregates: staging built");

    finalize(client, "assets", staging, dry_run).await?;

    info!(rows, dry_run, "asset_aggregates: completed");
    Ok(AssetAggregatesStats {
        assets_rows: rows,
        dry_run,
    })
}

async fn finalize(
    client: &ClickhouseClient,
    live: &str,
    staging: &str,
    dry_run: bool,
) -> Result<(), BackfillError> {
    if dry_run {
        debug!(live, staging, "asset_aggregates: dry-run, dropping staging");
        drop_if_exists(client, staging).await?;
        return Ok(());
    }
    client
        .query(&format!("EXCHANGE TABLES {live} AND {staging}"))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    info!(live, staging, "asset_aggregates: tables exchanged");
    drop_if_exists(client, staging).await?;
    Ok(())
}

async fn create_staging_like(
    client: &ClickhouseClient,
    source: &str,
    staging: &str,
) -> Result<(), BackfillError> {
    let sql = format!("CREATE TABLE {staging} AS {source}");
    client
        .query(&sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    debug!(source, staging, "asset_aggregates: staging table created");
    Ok(())
}

async fn drop_if_exists(client: &ClickhouseClient, table: &str) -> Result<(), BackfillError> {
    let sql = format!("DROP TABLE IF EXISTS {table}");
    client
        .query(&sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

async fn staging_row_count(client: &ClickhouseClient, table: &str) -> Result<u64, BackfillError> {
    let n: u64 = client
        .query(&format!("SELECT count() FROM {table}"))
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::Sink;

    #[tokio::test]
    async fn pg_target_short_circuits() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://noop")
            .expect("lazy connect must succeed without I/O");
        let sink = Sink::Postgres(pool);
        let stats = execute(&sink, false)
            .await
            .expect("PG short-circuit must not error");
        assert_eq!(stats.assets_rows, 0);
    }
}
