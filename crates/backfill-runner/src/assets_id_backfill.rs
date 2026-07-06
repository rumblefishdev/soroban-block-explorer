//! Task 0331 — one-shot backfill of `assets.id` for rows written before the `id`
//! column existed. After `ALTER TABLE assets ADD COLUMN id Int64 DEFAULT 0`, every
//! pre-existing row has `id = 0`; the unified reads join `assets.id = balances.asset_id`
//! and the classic→`balances` migration selects `assets.id`, so an un-backfilled
//! table means empty supply / portfolios and orphaned migrated balances.
//!
//! `id = ids::asset_id(...)` is a **Rust** cityhash — CH `cityHash64` differs, so it
//! CANNOT be computed in SQL (a SQL-computed id would never match the Rust-keyed
//! `balances`). This pass computes it in Rust and swaps it in.
//!
//! ## Mechanism (mirrors `contract_type_rebuild`)
//!
//! `assets` is a `ReplacingMergeTree` with NO version column, so a plain re-INSERT
//! can't deterministically override a row. Instead:
//!
//! 1. Read every identity 4-tuple from `assets FINAL`.
//! 2. Compute `id` in Rust (`ids::asset_id`, the SAME fn the live writer's
//!    `AssetRow::staged` uses) into a temp map table.
//! 3. Build a staging `assets` via `INSERT … SELECT … LEFT JOIN` the map, overriding
//!    only `id`; every other column passes through.
//! 4. `EXCHANGE TABLES` (atomic) and drop the temp.
//!
//! Idempotent (recomputes deterministically). `--dry-run` builds staging, reports,
//! then drops it — the live table is untouched.
//!
//! **Operational:** run with the indexer STOPPED — `EXCHANGE` swaps the whole table,
//! so a concurrent live write between staging-build and swap would be lost. The
//! `id_zero_after` stat MUST be 0 on a for-real run (else a row escaped the map).

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use db_clickhouse::persist::ids;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::ch_staging::{create_staging_like, drop_if_exists, finalize};
use crate::error::BackfillError;
use crate::sink::Sink;
use crate::util::insert_rows;

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetsIdBackfillStats {
    /// Rows in `assets FINAL` at read time.
    pub total_rows: u64,
    /// Rows with `id = 0` before the swap.
    pub id_zero_before: u64,
    /// Rows with `id = 0` after the swap — MUST be 0 on a for-real run.
    pub id_zero_after: u64,
    pub dry_run: bool,
}

/// Identity 4-tuple read from `assets`.
#[derive(Row, Deserialize)]
struct IdentityRow {
    asset_type: i16,
    asset_code: String,
    issuer_id: i64,
    contract_id: i64,
}

/// Row written to the temp map table: identity + its Rust-computed surrogate.
#[derive(Row, Serialize)]
struct IdMapRow {
    asset_type: i16,
    asset_code: String,
    issuer_id: i64,
    contract_id: i64,
    id: i64,
}

const MAP_TABLE: &str = "assets_id_map_0331";
const STAGING_TABLE: &str = "assets_staging_id_0331";

pub async fn execute(sink: &Sink, dry_run: bool) -> Result<AssetsIdBackfillStats, BackfillError> {
    let client = sink.client();

    let mut stats = AssetsIdBackfillStats {
        dry_run,
        ..Default::default()
    };

    // ---- Phase 1: read identity 4-tuples, compute `id` in Rust ----
    let rows = client
        .query("SELECT asset_type, asset_code, issuer_id, contract_id FROM assets FINAL")
        .fetch_all::<IdentityRow>()
        .await
        .map_err(BackfillError::Ch)?;
    stats.total_rows = rows.len() as u64;
    stats.id_zero_before = count_id_zero(client).await?;

    let map: Vec<IdMapRow> = rows
        .into_iter()
        .map(|r| {
            let id = ids::asset_id(r.asset_type, &r.asset_code, r.issuer_id, r.contract_id);
            IdMapRow {
                asset_type: r.asset_type,
                asset_code: r.asset_code,
                issuer_id: r.issuer_id,
                contract_id: r.contract_id,
                id,
            }
        })
        .collect();

    // ---- Phase 2: push the map to a temp table for the SQL join ----
    drop_if_exists(client, MAP_TABLE).await?;
    create_map_table(client, MAP_TABLE).await?;
    insert_rows(client, MAP_TABLE, &map).await?;

    // ---- Phase 3: build staging `assets` with `id` overridden from the map ----
    drop_if_exists(client, STAGING_TABLE).await?;
    create_staging_like(client, "assets", STAGING_TABLE).await?;
    build_staging(client, STAGING_TABLE, MAP_TABLE).await?;

    // ---- Phase 4: swap (or drop staging on dry-run) + drop the map ----
    if dry_run {
        finalize(client, "assets", STAGING_TABLE, true).await?;
        drop_if_exists(client, MAP_TABLE).await?;
        // Every identity read was mapped, so a real run leaves 0. Report it as such.
        stats.id_zero_after = 0;
    } else {
        finalize(client, "assets", STAGING_TABLE, false).await?;
        drop_if_exists(client, MAP_TABLE).await?;
        stats.id_zero_after = count_id_zero(client).await?;
    }

    info!(
        total_rows = stats.total_rows,
        id_zero_before = stats.id_zero_before,
        id_zero_after = stats.id_zero_after,
        dry_run,
        "assets_id_backfill: completed"
    );
    Ok(stats)
}

async fn create_map_table(client: &ClickhouseClient, table: &str) -> Result<(), BackfillError> {
    client
        .query(&format!(
            "CREATE TABLE {table} \
             (asset_type Int16, asset_code String, issuer_id Int64, contract_id Int64, id Int64) \
             ENGINE = Memory"
        ))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

/// Passthrough every `assets` column via `a.* REPLACE`, overriding only `id` — no
/// hardcoded column list, so an `assets` schema change can't silently misalign the
/// staged rows before the `EXCHANGE`. `toString` on the `LowCardinality(String)`
/// `asset_code` so it compares to the plain-`String` map key. `if(m.id != 0, …)`
/// keeps any already-correct id on a LEFT-JOIN miss (e.g. a row a newer indexer
/// already stamped) instead of zeroing it.
async fn build_staging(
    client: &ClickhouseClient,
    staging: &str,
    map: &str,
) -> Result<(), BackfillError> {
    let sql = format!(
        "INSERT INTO {staging} \
         SELECT a.* REPLACE (if(m.id != 0, m.id, a.id) AS id) \
         FROM assets AS a FINAL \
         LEFT JOIN {map} AS m \
           ON  m.asset_type  = a.asset_type \
           AND m.asset_code  = toString(a.asset_code) \
           AND m.issuer_id   = a.issuer_id \
           AND m.contract_id = a.contract_id"
    );
    client
        .query(&sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

async fn count_id_zero(client: &ClickhouseClient) -> Result<u64, BackfillError> {
    let n: u64 = client
        .query("SELECT count() FROM assets FINAL WHERE id = 0")
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end against a real ClickHouse in a throwaway database (the run does a
    /// whole-table `EXCHANGE` on `assets`, so it must NEVER touch a shared table).
    /// Gated on `CLICKHOUSE_URL`.
    ///
    ///   docker compose up -d clickhouse
    ///   CLICKHOUSE_URL=http://localhost:8123 \
    ///       cargo test -p backfill-runner assets_id_backfill
    #[tokio::test]
    async fn backfill_fills_id_via_rust_hash_e2e() {
        use db_clickhouse::{Config, apply_init_sql, client};

        let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
            eprintln!("CLICKHOUSE_URL not set — skipping assets_id_backfill e2e");
            return;
        };
        let db = "ch_test_0331_assets_id";
        let base = client(&Config {
            url: url.clone(),
            ..Config::from_env()
        });
        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("drop pre-existing throwaway db");
        base.query(&format!("CREATE DATABASE {db}"))
            .execute()
            .await
            .expect("create throwaway db");

        let cl = client(&Config {
            url,
            database: db.to_string(),
            ..Config::from_env()
        });
        apply_init_sql(&cl).await.expect("apply init schema");

        // A native, a classic USDC, and a soroban token — all inserted with id=0.
        let usdc_issuer = ids::account_id("GISSUER");
        let token_contract = ids::contract_id("CTOKEN1");
        cl.query(&format!(
            "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, id) VALUES \
             (0, '', 0, 0, 0), \
             (1, 'USDC', {usdc_issuer}, 0, 0), \
             (3, '', 0, {token_contract}, 0)"
        ))
        .execute()
        .await
        .expect("seed assets");

        let sink = Sink::new(cl.clone());
        let stats = execute(&sink, false).await.expect("backfill run");
        assert_eq!(stats.total_rows, 3);
        assert_eq!(stats.id_zero_before, 3);
        assert_eq!(stats.id_zero_after, 0, "every row backfilled");

        let id_of = |asset_type: i16, code: &'static str, issuer: i64, contract: i64| {
            let cl = cl.clone();
            async move {
                cl.query(
                    "SELECT id FROM assets FINAL \
                     WHERE asset_type = ? AND asset_code = ? AND issuer_id = ? AND contract_id = ?",
                )
                .bind(asset_type)
                .bind(code)
                .bind(issuer)
                .bind(contract)
                .fetch_one::<i64>()
                .await
                .expect("id row")
            }
        };
        assert_eq!(
            id_of(0, "", 0, 0).await,
            ids::asset_id(0, "", 0, 0),
            "native"
        );
        assert_eq!(
            id_of(1, "USDC", usdc_issuer, 0).await,
            ids::asset_id(1, "USDC", usdc_issuer, 0),
            "classic"
        );
        assert_eq!(
            id_of(3, "", 0, token_contract).await,
            ids::asset_id(3, "", 0, token_contract),
            "soroban == its contract surrogate"
        );

        // Idempotent: a re-run leaves the ids unchanged and still 0 zeros.
        let stats2 = execute(&sink, false).await.expect("idempotent re-run");
        assert_eq!(stats2.id_zero_after, 0);
        assert_eq!(
            id_of(1, "USDC", usdc_issuer, 0).await,
            ids::asset_id(1, "USDC", usdc_issuer, 0),
            "classic id stable on re-run"
        );

        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("cleanup throwaway db");
    }
}
