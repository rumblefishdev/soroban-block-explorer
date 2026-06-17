//! Task 0283 — one-shot rebuild of `soroban_contracts.contract_type` from
//! `wasm_interface_metadata`, plus the `assets` type-3 (Soroban-fungible)
//! backfill that cascades from it.
//!
//! ## Why this exists
//!
//! The CH writer only stamps a `Nft`/`Fungible` verdict on a deploy when the
//! WASM was uploaded in the SAME ledger (the live G1 fix now also resolves the
//! common cross-ledger case going forward). Historical contracts deployed
//! before the fix sit at the parser default `Other`, so the whole NFT promotion
//! machinery (`nft-reclassify`) is a no-op. This pass reconstructs the verdict
//! for the entire history in one shot.
//!
//! ## Mechanism (mirrors `repair_tier1` / `asset_aggregates`)
//!
//! ClickHouse has no cheap in-place UPDATE and the RMT version column
//! (`wasm_uploaded_at_ledger`) is real data that must not be bumped, so a
//! plain re-INSERT cannot deterministically override existing rows. Instead:
//!
//! 1. Classify every WASM in **Rust** (`classify_contract_from_wasm_spec`,
//!    exact parity — no SQL rule duplication) into a temp verdict table.
//! 2. Build a staging `soroban_contracts` via `INSERT … SELECT … LEFT JOIN`
//!    the verdicts, overriding `contract_type` only for non-SAC contracts that
//!    resolve to `Nft`/`Fungible`; everything else passes through unchanged.
//! 3. `EXCHANGE TABLES` (atomic, ~0.1 s) and drop the temps.
//! 4. Backfill the `assets` type-3 rows for the now-`Fungible` contracts
//!    (parity with PG `insert_assets_from_reclassified_contracts`).
//!
//! Idempotent. `--dry-run` reports the verdict transitions + would-be asset
//! inserts without touching the live tables.
//!
//! **Operational:** run with the indexer STOPPED — `EXCHANGE` swaps the whole
//! table, so a concurrent live write between staging-build and swap would be
//! lost. Must run BEFORE `nft-reclassify` (which reads `contract_type = 2`).

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use xdr_parser::types::ContractFunction;
use xdr_parser::{ContractClassification, classify_contract_from_wasm_spec};

use crate::ch_staging::{create_staging_like, drop_if_exists, exchange_tables};
use crate::error::BackfillError;
use crate::sink::Sink;

/// `ContractType::Nft = 2`.
const CONTRACT_TYPE_NFT: i16 = 2;
/// `ContractType::Fungible = 3`.
const CONTRACT_TYPE_FUNGIBLE: i16 = 3;

#[derive(Debug, Default, Clone, Copy)]
pub struct ContractTypeRebuildStats {
    /// Contracts whose verdict flips to `Nft` (vs the pre-rebuild value).
    pub flipped_nft: u64,
    /// Contracts whose verdict flips to `Fungible`.
    pub flipped_fungible: u64,
    /// `assets` type-3 rows that were (or would be, dry-run) inserted.
    pub assets_inserted: u64,
    pub dry_run: bool,
}

/// `(wasm_hash, metadata)` row read from `wasm_interface_metadata`.
#[derive(Row, Deserialize)]
struct WasmMetaRow {
    wasm_hash: [u8; 32],
    metadata: String,
}

/// JSON shape of `wasm_interface_metadata.metadata` (the `functions` slice is
/// all the classifier reads); written by the stage as
/// `{"functions":[…],"wasm_byte_len":N}`.
#[derive(Deserialize)]
struct WasmMetadata {
    functions: Vec<ContractFunction>,
}

/// Row written to the temp verdict table, keyed by raw `wasm_hash`.
#[derive(Row, Serialize)]
struct VerdictRow {
    wasm_hash: [u8; 32],
    verdict: i16,
}

const VERDICT_TABLE: &str = "wasm_verdicts_staging_0283";
const STAGING_TABLE: &str = "soroban_contracts_staging_rebuild";

pub async fn execute(
    sink: &Sink,
    dry_run: bool,
) -> Result<ContractTypeRebuildStats, BackfillError> {
    let Sink::Clickhouse(client) = sink else {
        info!("contract_type_rebuild: skipped (PG target — CH-only maintenance)");
        return Ok(ContractTypeRebuildStats::default());
    };

    let mut stats = ContractTypeRebuildStats {
        dry_run,
        ..Default::default()
    };

    // ---- Phase 1: classify every WASM in Rust (parity) ----
    let verdicts = classify_all_wasm(client).await?;
    info!(
        verdicts = verdicts.len(),
        "contract_type_rebuild: classified WASM (Nft/Fungible only)"
    );

    // ---- Phase 2: push verdicts to a temp table for the SQL join ----
    drop_if_exists(client, VERDICT_TABLE).await?;
    create_verdict_table(client, VERDICT_TABLE).await?;
    insert_verdicts(client, VERDICT_TABLE, &verdicts).await?;

    // ---- Phase 3: build the rebuilt `soroban_contracts` into staging ----
    drop_if_exists(client, STAGING_TABLE).await?;
    create_staging_like(client, "soroban_contracts", STAGING_TABLE).await?;
    build_staging(client, STAGING_TABLE, VERDICT_TABLE).await?;

    // Count verdict transitions vs the still-live table (works pre-EXCHANGE).
    let (flipped_nft, flipped_fungible) = count_flips(client, STAGING_TABLE).await?;
    stats.flipped_nft = flipped_nft;
    stats.flipped_fungible = flipped_fungible;
    // assets that would be inserted (read from staging, which has the new type-3).
    let assets_would_insert = count_assets_to_insert(client, STAGING_TABLE).await?;

    if dry_run {
        debug!("contract_type_rebuild: dry-run — dropping staging, no swap/insert");
        drop_if_exists(client, STAGING_TABLE).await?;
        drop_if_exists(client, VERDICT_TABLE).await?;
        stats.assets_inserted = assets_would_insert;
    } else {
        // ---- Phase 4: atomic swap ----
        exchange_tables(client, "soroban_contracts", STAGING_TABLE).await?;
        drop_if_exists(client, STAGING_TABLE).await?;
        drop_if_exists(client, VERDICT_TABLE).await?;
        // ---- Phase 5: assets type-3 backfill (contract_type now authoritative)
        stats.assets_inserted = backfill_assets(client).await?;
    }

    info!(
        flipped_nft = stats.flipped_nft,
        flipped_fungible = stats.flipped_fungible,
        assets_inserted = stats.assets_inserted,
        dry_run,
        "contract_type_rebuild: completed"
    );
    Ok(stats)
}

/// Read all `wasm_interface_metadata`, classify each hash in Rust, and return
/// only the definitive `Nft`/`Fungible` verdicts (Other is the parser default,
/// nothing to override).
async fn classify_all_wasm(client: &ClickhouseClient) -> Result<Vec<VerdictRow>, BackfillError> {
    let rows = client
        .query("SELECT wasm_hash, metadata FROM wasm_interface_metadata")
        .fetch_all::<WasmMetaRow>()
        .await
        .map_err(BackfillError::Ch)?;

    let mut out = Vec::new();
    for row in rows {
        let Ok(meta) = serde_json::from_str::<WasmMetadata>(&row.metadata) else {
            continue;
        };
        // Only Nft/Fungible are worth overriding `Other` (the parser default).
        let verdict = match classify_contract_from_wasm_spec(&meta.functions) {
            ContractClassification::Nft => CONTRACT_TYPE_NFT,
            ContractClassification::Fungible => CONTRACT_TYPE_FUNGIBLE,
            _ => continue,
        };
        out.push(VerdictRow {
            wasm_hash: row.wasm_hash,
            verdict,
        });
    }
    Ok(out)
}

async fn create_verdict_table(client: &ClickhouseClient, table: &str) -> Result<(), BackfillError> {
    client
        .query(&format!(
            "CREATE TABLE {table} (wasm_hash FixedString(32), verdict Int16) ENGINE = Memory"
        ))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

async fn insert_verdicts(
    client: &ClickhouseClient,
    table: &str,
    verdicts: &[VerdictRow],
) -> Result<(), BackfillError> {
    if verdicts.is_empty() {
        return Ok(());
    }
    let mut insert = client
        .insert::<VerdictRow>(table)
        .await
        .map_err(BackfillError::Ch)?;
    for v in verdicts {
        insert.write(v).await.map_err(BackfillError::Ch)?;
    }
    insert.end().await.map_err(BackfillError::Ch)?;
    Ok(())
}

/// Build the staging table: passthrough every column, override `contract_type`
/// only for non-SAC contracts whose WASM resolved to Nft/Fungible.
async fn build_staging(
    client: &ClickhouseClient,
    staging: &str,
    verdict_tbl: &str,
) -> Result<(), BackfillError> {
    let sql = format!(
        "INSERT INTO {staging} \
         SELECT \
           sc.id, sc.contract_id, sc.wasm_hash, sc.wasm_uploaded_at_ledger, \
           sc.deployer_id, sc.deployed_at_ledger, \
           if(NOT sc.is_sac AND v.verdict IN (2, 3), v.verdict, sc.contract_type) AS contract_type, \
           sc.is_sac, sc.name \
         FROM soroban_contracts AS sc FINAL \
         LEFT JOIN {verdict_tbl} AS v ON v.wasm_hash = sc.wasm_hash"
    );
    client
        .query(&sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

/// Count contracts whose `contract_type` actually changes to Nft / Fungible.
async fn count_flips(
    client: &ClickhouseClient,
    staging: &str,
) -> Result<(u64, u64), BackfillError> {
    #[derive(Row, Deserialize)]
    struct FlipRow {
        nft: u64,
        fungible: u64,
    }
    // `ifNull(old, -1)` so a flip FROM NULL (unclassified, ~4,875 on the
    // snapshot) counts — a plain `s != old` returns NULL for NULL `old` and
    // would silently drop those flips from the dry-run report.
    let sql = format!(
        "SELECT \
           countIf(s.contract_type = 2) AS nft, \
           countIf(s.contract_type = 3) AS fungible \
         FROM {staging} AS s \
         INNER JOIN soroban_contracts AS old FINAL ON old.contract_id = s.contract_id \
         WHERE ifNull(old.contract_type, -1) != s.contract_type"
    );
    let r = client
        .query(&sql)
        .fetch_one::<FlipRow>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok((r.nft, r.fungible))
}

/// Count `assets` type-3 rows that would be inserted from the staged verdicts.
async fn count_assets_to_insert(
    client: &ClickhouseClient,
    staging: &str,
) -> Result<u64, BackfillError> {
    let sql = format!(
        "SELECT count() FROM {staging} AS s \
         WHERE s.contract_type = 3 AND NOT s.is_sac \
           AND NOT EXISTS ( \
                 SELECT 1 FROM assets AS a FINAL \
                  WHERE a.contract_id = s.id AND a.asset_type IN (2, 3) \
               )"
    );
    let n: u64 = client
        .query(&sql)
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

/// Insert the missing type-3 (Soroban-fungible) `assets` rows from the now-live
/// `soroban_contracts`. Mirrors PG `insert_assets_from_reclassified_contracts`:
/// only `asset_type` + `contract_id`, guarded against an existing SAC/Soroban row.
async fn backfill_assets(client: &ClickhouseClient) -> Result<u64, BackfillError> {
    let before = assets_type3_count(client).await?;
    let sql = "INSERT INTO assets \
         (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url) \
         SELECT 3, '', 0, sc.id, NULL, NULL, NULL, NULL \
         FROM soroban_contracts AS sc FINAL \
         WHERE sc.contract_type = 3 AND NOT sc.is_sac \
           AND NOT EXISTS ( \
                 SELECT 1 FROM assets AS a FINAL \
                  WHERE a.contract_id = sc.id AND a.asset_type IN (2, 3) \
               )";
    client
        .query(sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    let after = assets_type3_count(client).await?;
    Ok(after.saturating_sub(before))
}

async fn assets_type3_count(client: &ClickhouseClient) -> Result<u64, BackfillError> {
    let n: u64 = client
        .query("SELECT count() FROM assets FINAL WHERE asset_type = 3")
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(stats.flipped_nft, 0);
        assert_eq!(stats.assets_inserted, 0);
    }

    /// End-to-end against a real ClickHouse — exercises the whole pipeline the
    /// operators actually run (classify → staging → EXCHANGE swap → assets
    /// backfill) and the SQL the unit tests can't reach (`hex()` case-fold,
    /// `FINAL` joins, `NOT EXISTS` guard, EXCHANGE).
    ///
    /// Gated on `CLICKHOUSE_URL` (skips cleanly when unset, like `persist_e2e`).
    /// **Isolated**: runs entirely inside a throwaway database it creates and
    /// drops, because `execute` does a whole-table `EXCHANGE` on
    /// `soroban_contracts` and must NEVER touch a real table — even if
    /// `CLICKHOUSE_URL` points at a shared server.
    ///
    ///   docker compose up -d clickhouse
    ///   CLICKHOUSE_URL=http://localhost:8123 \
    ///       cargo test -p backfill-runner contract_type_rebuild
    #[tokio::test]
    async fn rebuild_e2e_flips_other_and_backfills_assets() {
        use db_clickhouse::{Config, apply_init_sql, client};

        let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
            eprintln!("CLICKHOUSE_URL not set — skipping contract_type_rebuild e2e");
            return;
        };
        let db = "ch_test_0283_rebuild";
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

        // Seed: a definitive-Nft WASM (owner_of) + a definitive-Fungible WASM
        // (total_supply), and three contracts — two non-SAC at the parser
        // default `Other` (1) whose WASM should flip them, plus a SAC (Token=0)
        // that must stay untouched.
        let nft_hash = "11".repeat(32); // 64 hex chars = FixedString(32)
        let fun_hash = "22".repeat(32);
        cl.query(&format!(
            "INSERT INTO wasm_interface_metadata (wasm_hash, metadata) VALUES \
             (unhex('{nft_hash}'), '{{\"functions\":[{{\"name\":\"owner_of\",\"doc\":\"\",\"inputs\":[],\"outputs\":[]}}],\"wasm_byte_len\":1}}'), \
             (unhex('{fun_hash}'), '{{\"functions\":[{{\"name\":\"total_supply\",\"doc\":\"\",\"inputs\":[],\"outputs\":[]}}],\"wasm_byte_len\":1}}')"
        ))
        .execute()
        .await
        .expect("seed wasm_interface_metadata");

        cl.query(&format!(
            "INSERT INTO soroban_contracts \
             (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac, name) VALUES \
             (1001, 'CNFT', unhex('{nft_hash}'), 100, NULL, 100, 1, false, NULL), \
             (1002, 'CFUN', unhex('{fun_hash}'), 100, NULL, 100, 1, false, NULL), \
             (1003, 'CSAC', NULL, 0, NULL, NULL, 0, true, NULL)"
        ))
        .execute()
        .await
        .expect("seed soroban_contracts");

        let sink = Sink::Clickhouse(cl.clone());
        let stats = execute(&sink, false).await.expect("rebuild run");
        assert_eq!(stats.flipped_nft, 1, "one Other -> Nft");
        assert_eq!(stats.flipped_fungible, 1, "one Other -> Fungible");
        assert_eq!(stats.assets_inserted, 1, "one type-3 asset for the fungible");

        let verdict = |cid: &'static str| {
            let cl = cl.clone();
            async move {
                // `contract_type` is Nullable(Int16) → Option<i16>.
                cl.query("SELECT contract_type FROM soroban_contracts FINAL WHERE contract_id = ?")
                    .bind(cid)
                    .fetch_one::<Option<i16>>()
                    .await
                    .expect("verdict row")
                    .expect("verdict is non-null")
            }
        };
        assert_eq!(verdict("CNFT").await, 2, "Nft verdict written");
        assert_eq!(verdict("CFUN").await, 3, "Fungible verdict written");
        assert_eq!(verdict("CSAC").await, 0, "SAC untouched");

        let type3 = |contract_id: i64| {
            let cl = cl.clone();
            async move {
                cl.query("SELECT count() FROM assets FINAL WHERE asset_type = 3 AND contract_id = ?")
                    .bind(contract_id)
                    .fetch_one::<u64>()
                    .await
                    .expect("assets count")
            }
        };
        assert_eq!(type3(1002).await, 1, "Fungible gets a type-3 asset");
        assert_eq!(type3(1001).await, 0, "Nft gets NO asset row");

        // Idempotent: a second run flips nothing new and inserts no asset dup.
        let stats2 = execute(&sink, false).await.expect("idempotent re-run");
        assert_eq!(stats2.flipped_nft, 0, "re-run flips nothing");
        assert_eq!(stats2.flipped_fungible, 0, "re-run flips nothing");
        assert_eq!(stats2.assets_inserted, 0, "re-run inserts no asset");
        assert_eq!(type3(1002).await, 1, "still exactly one type-3 asset");

        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("cleanup throwaway db");
    }
}
