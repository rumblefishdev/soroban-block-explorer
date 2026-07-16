//! Task 0320 — one-shot backfill of `soroban_contracts.wasm_hash` for contracts
//! that upgraded their WASM (`update_current_contract_wasm`).
//!
//! ## Why this exists
//!
//! The parser only ever recorded a contract's `wasm_hash` at deploy time, so a
//! contract that later upgrades keeps its stale deploy-time hash forever — the
//! API then serves the wrong code hash AND the wrong interface (`fetch_wasm_interface`
//! joins `wasm_hash → wasm_interface_metadata`). Measured on prod 2026-06-24:
//! 1,362 contracts affected. Full research in lore task 0320.
//!
//! ## Mechanism (mirrors `contract_type_rebuild` / `repair_tier1`)
//!
//! The fix signal — the `["executable_update", old, new]` SYSTEM event — is
//! already ingested into `soroban_events` (decoded typed-JSON), so this is a
//! pure in-CH rebuild, NO S3 re-parse:
//!
//! 1. Read the LATEST `executable_update` per contract (`argMax` by ledger),
//!    parse the NEW wasm hash out of the topics in Rust
//!    (`xdr_parser::event::extract_executable_update_new_wasm_hash`).
//! 2. Push `(contract_id, new_wasm_hash, upgrade_ledger)` to a temp table.
//! 3. Build a staging `soroban_contracts` via `INSERT … SELECT … LEFT JOIN` that
//!    overrides `wasm_hash` + `wasm_uploaded_at_ledger` (the RMT version) for the
//!    upgraded contracts; everything else passes through unchanged.
//! 4. `EXCHANGE TABLES` (atomic) and drop the temp.
//!
//! `contract_type` is intentionally left untouched: the class never net-changes
//! across an upgrade on current mainnet data (0 of 1,362 — see 0320), so the
//! existing verdict is already correct. The rare class-flip handling (reclassify
//! + NFT quarantine promote/drop) is deferred to task 0325.
//!
//! Idempotent. `--dry-run` reports the would-be corrections without writing.
//!
//! **Operational:** run with the indexer STOPPED — `EXCHANGE` swaps the whole
//! table, so a concurrent live write between staging-build and swap would be lost.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use xdr_parser::event::extract_executable_update_new_wasm_hash;

use crate::ch_staging::{create_staging_like, drop_if_exists, finalize};
use crate::error::BackfillError;
use crate::sink::Sink;

#[derive(Debug, Default, Clone, Copy)]
pub struct WasmUpgradeBackfillStats {
    /// Contracts with at least one `executable_update` event.
    pub upgraded_contracts: u64,
    /// Of those, how many had a stale `wasm_hash` actually corrected.
    pub corrected: u64,
    /// Events whose topics failed to parse a new wasm hash (skipped).
    pub unparseable: u64,
    pub dry_run: bool,
}

/// `(contract_id, latest topics, latest ledger)` from `soroban_events`.
#[derive(Row, Deserialize)]
struct LatestUpgradeEventRow {
    contract_id: i64,
    topics_xdr: String,
    upgrade_ledger: i64,
}

/// Row written to the temp override table, keyed by `soroban_contracts.id`.
#[derive(Row, Serialize)]
struct UpgradeRow {
    contract_id: i64,
    wasm_hash: [u8; 32],
    upgrade_ledger: i64,
}

const UPGRADE_TABLE: &str = "wasm_upgrades_staging_0320";
const STAGING_TABLE: &str = "soroban_contracts_staging_wasm_upgrade";

pub async fn execute(
    sink: &Sink,
    dry_run: bool,
) -> Result<WasmUpgradeBackfillStats, BackfillError> {
    let client = sink.client();

    let mut stats = WasmUpgradeBackfillStats {
        dry_run,
        ..Default::default()
    };

    // ---- Phase 1: latest executable_update per contract → parse new hash ----
    let upgrades = collect_latest_upgrades(client, &mut stats).await?;
    stats.upgraded_contracts = upgrades.len() as u64;
    info!(
        upgraded_contracts = stats.upgraded_contracts,
        unparseable = stats.unparseable,
        "wasm_upgrade_backfill: collected latest upgrades"
    );
    if upgrades.is_empty() {
        return Ok(stats);
    }

    // ---- Phase 2: push to a temp table for the SQL join ----
    drop_if_exists(client, UPGRADE_TABLE).await?;
    create_upgrade_table(client, UPGRADE_TABLE).await?;
    insert_upgrades(client, UPGRADE_TABLE, &upgrades).await?;

    // ---- Phase 3: build the corrected `soroban_contracts` into staging ----
    drop_if_exists(client, STAGING_TABLE).await?;
    create_staging_like(client, "soroban_contracts", STAGING_TABLE).await?;
    build_staging(client, STAGING_TABLE, UPGRADE_TABLE).await?;

    // How many stored hashes actually differ from the chain-current (pre-swap).
    stats.corrected = count_corrected(client, UPGRADE_TABLE).await?;

    // ---- Phase 4: atomic swap (indexer STOPPED) or drop (dry-run) ----
    finalize(client, "soroban_contracts", STAGING_TABLE, dry_run).await?;
    drop_if_exists(client, UPGRADE_TABLE).await?;

    info!(
        upgraded_contracts = stats.upgraded_contracts,
        corrected = stats.corrected,
        unparseable = stats.unparseable,
        dry_run,
        "wasm_upgrade_backfill: completed"
    );
    Ok(stats)
}

/// Read the latest `executable_update` event per contract and parse the new
/// wasm hash out of each topics blob. Events whose topics don't parse are
/// counted in `stats.unparseable` and skipped (never silently corrupt a row).
async fn collect_latest_upgrades(
    client: &ClickhouseClient,
    stats: &mut WasmUpgradeBackfillStats,
) -> Result<Vec<UpgradeRow>, BackfillError> {
    let rows = client
        .query(
            // Tie-break within a ledger by event_index so two upgrades in the
            // same ledger pick the LAST-applied (matches the live path's
            // application-order collapse), not an arbitrary tied row.
            // `event_type = 0` = SYSTEM (domain::ContractEventType::System). Only
            // the host emits `executable_update`, always as a System event; a
            // contract could emit a Contract-typed (event_type = 1) event with
            // the same topic shape, so restrict to System to avoid a spoofed
            // wasm_hash. Mirrors the live path's guard.
            "SELECT contract_id, \
                    argMax(topics_xdr, (ledger_sequence, event_index)) AS topics_xdr, \
                    max(ledger_sequence) AS upgrade_ledger \
             FROM soroban_events \
             WHERE signature = 'executable_update' AND event_type = 0 \
             GROUP BY contract_id",
        )
        .fetch_all::<LatestUpgradeEventRow>()
        .await
        .map_err(BackfillError::Ch)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let parsed = serde_json::from_str::<Value>(&row.topics_xdr)
            .ok()
            .and_then(|t| extract_executable_update_new_wasm_hash(&t));
        match parsed {
            Some(wasm_hash) => out.push(UpgradeRow {
                contract_id: row.contract_id,
                wasm_hash,
                upgrade_ledger: row.upgrade_ledger,
            }),
            None => {
                stats.unparseable += 1;
                warn!(
                    contract_id = row.contract_id,
                    "wasm_upgrade_backfill: could not parse new wasm hash from executable_update topics"
                );
            }
        }
    }
    Ok(out)
}

async fn create_upgrade_table(client: &ClickhouseClient, table: &str) -> Result<(), BackfillError> {
    client
        .query(&format!(
            "CREATE TABLE {table} \
             (contract_id Int64, wasm_hash FixedString(32), upgrade_ledger Int64) \
             ENGINE = Memory"
        ))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

async fn insert_upgrades(
    client: &ClickhouseClient,
    table: &str,
    upgrades: &[UpgradeRow],
) -> Result<(), BackfillError> {
    let mut insert = client
        .insert::<UpgradeRow>(table)
        .await
        .map_err(BackfillError::Ch)?;
    for u in upgrades {
        insert.write(u).await.map_err(BackfillError::Ch)?;
    }
    insert.end().await.map_err(BackfillError::Ch)?;
    Ok(())
}

/// Build the staging table: passthrough every column, override `wasm_hash` +
/// `wasm_uploaded_at_ledger` only for contracts present in the upgrade temp.
/// `upgrade_ledger > 0` distinguishes a matched LEFT JOIN row from the default.
async fn build_staging(
    client: &ClickhouseClient,
    staging: &str,
    upgrade_tbl: &str,
) -> Result<(), BackfillError> {
    let sql = format!(
        "INSERT INTO {staging} \
         SELECT \
           sc.id, sc.contract_id, \
           if(u.upgrade_ledger > 0, u.wasm_hash, sc.wasm_hash) AS wasm_hash, \
           if(u.upgrade_ledger > 0, u.upgrade_ledger, sc.wasm_uploaded_at_ledger) AS wasm_uploaded_at_ledger, \
           sc.deployer_id, sc.deployed_at_ledger, sc.contract_type, sc.is_sac \
         FROM soroban_contracts AS sc FINAL \
         LEFT JOIN {upgrade_tbl} AS u ON u.contract_id = sc.id"
    );
    client
        .query(&sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(())
}

/// Count contracts whose stored `wasm_hash` actually differs from the
/// chain-current hash (i.e. the genuinely-stale rows being corrected).
async fn count_corrected(
    client: &ClickhouseClient,
    upgrade_tbl: &str,
) -> Result<u64, BackfillError> {
    let sql = format!(
        "SELECT count() FROM {upgrade_tbl} AS u \
         INNER JOIN soroban_contracts AS sc FINAL ON sc.id = u.contract_id \
         WHERE sc.wasm_hash IS NULL OR sc.wasm_hash != u.wasm_hash"
    );
    let n: u64 = client
        .query(&sql)
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end against a real ClickHouse — seed a stale `soroban_contracts`
    /// row + its `executable_update` event, run the backfill, assert the hash is
    /// corrected to the event's new hash and a non-upgraded contract is untouched.
    ///
    /// Gated on `CLICKHOUSE_URL` (skips cleanly when unset). Isolated in a
    /// throwaway database because `execute` does a whole-table `EXCHANGE`.
    ///
    ///   docker compose up -d clickhouse
    ///   CLICKHOUSE_URL=http://localhost:8123 \
    ///       cargo test -p backfill-runner wasm_upgrade_backfill
    #[tokio::test]
    async fn backfill_e2e_corrects_stale_hash() {
        use db_clickhouse::{Config, apply_init_sql, client};

        let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
            eprintln!("CLICKHOUSE_URL not set — skipping wasm_upgrade_backfill e2e");
            return;
        };
        let db = "ch_test_0320_wasm_upgrade";
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

        // Contract 1001 deployed at wasm 0x11..; later upgraded to 0x22.. (an
        // executable_update event carrying old=0x11.., new=0x22..). Contract
        // 1002 never upgraded — must stay at its deploy hash 0x33...
        let deploy_hash = "11".repeat(32);
        let new_hash_hex = "22".repeat(32);
        let untouched_hash = "33".repeat(32);
        // base64 of 32 × 0x22.
        let new_hash_b64 = base64_of_repeated_byte(0x22);

        cl.query(&format!(
            "INSERT INTO soroban_contracts \
             (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac) VALUES \
             (1001, 'CUPG', unhex('{deploy_hash}'), 100, NULL, 100, 1, false), \
             (1002, 'CNOUP', unhex('{untouched_hash}'), 100, NULL, 100, 1, false)"
        ))
        .execute()
        .await
        .expect("seed soroban_contracts");

        let topics = format!(
            r#"[{{"type":"sym","value":"executable_update"}},{{"type":"vec","value":[{{"type":"sym","value":"Wasm"}},{{"type":"bytes","value":"{old}"}}]}},{{"type":"vec","value":[{{"type":"sym","value":"Wasm"}},{{"type":"bytes","value":"{new}"}}]}}]"#,
            old = base64_of_repeated_byte(0x11),
            new = new_hash_b64,
        );
        cl.query(
            "INSERT INTO soroban_events \
             (contract_id, transaction_id, ledger_sequence, event_index, event_type, signature, topics_xdr, data_xdr) VALUES \
             (1001, 1, 500, 0, 0, 'executable_update', ?, '{\"type\":\"vec\",\"value\":[]}')",
        )
        .bind(&topics)
        .execute()
        .await
        .expect("seed soroban_events");

        let sink = Sink::new(cl.clone());
        let stats = execute(&sink, false).await.expect("backfill run");
        assert_eq!(stats.upgraded_contracts, 1);
        assert_eq!(stats.corrected, 1, "the one stale contract corrected");
        assert_eq!(stats.unparseable, 0);

        let hash_of = |cid: &'static str| {
            let cl = cl.clone();
            async move {
                cl.query("SELECT lower(hex(assumeNotNull(wasm_hash))) FROM soroban_contracts FINAL WHERE contract_id = ?")
                    .bind(cid)
                    .fetch_one::<String>()
                    .await
                    .expect("hash row")
            }
        };
        assert_eq!(hash_of("CUPG").await, new_hash_hex, "upgraded → new hash");
        assert_eq!(
            hash_of("CNOUP").await,
            untouched_hash,
            "non-upgraded untouched"
        );

        // Idempotent: a second run corrects nothing new.
        let stats2 = execute(&sink, false).await.expect("idempotent re-run");
        assert_eq!(stats2.corrected, 0, "re-run corrects nothing");

        base.query(&format!("DROP DATABASE IF EXISTS {db}"))
            .execute()
            .await
            .expect("cleanup throwaway db");
    }

    #[cfg(test)]
    fn base64_of_repeated_byte(b: u8) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([b; 32])
    }
}
