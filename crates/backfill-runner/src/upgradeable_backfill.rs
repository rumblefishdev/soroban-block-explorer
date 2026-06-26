//! Task 0327 — one-shot backfill of the `upgradeable` mutability bit into
//! `wasm_interface_metadata.metadata` for already-ingested WASMs.
//!
//! ## Why this exists
//!
//! The live parser writes `metadata.upgradeable` going forward, but every WASM
//! ingested before 0327 has no such key, so the API reads it as Unknown (the
//! contract page shows no chip). The mutability bit is derived from the WASM's
//! import table (does it import `update_current_contract_wasm`), which is NOT
//! stored in `metadata` (only `functions` + `wasm_byte_len` are) — so we must
//! re-read the raw WASM. Rather than an S3 re-parse, we fetch each distinct
//! WASM's current bytecode straight from Soroban RPC by `wasm_hash`.
//!
//! ## Mechanism
//!
//! 1. Read every `wasm_interface_metadata` row whose `metadata` lacks the
//!    `upgradeable` key (`NOT JSONHas`).
//! 2. Fetch the WASM per `wasm_hash` from Soroban RPC
//!    (`getLedgerEntries` / `LedgerKey::ContractCode`), batched.
//! 3. Run the SHIPPED parser (`xdr_parser::contract::wasm_imports_upgrade_fn`)
//!    on the bytecode, merge `upgradeable` into the existing metadata JSON.
//! 4. Re-INSERT the row. `wasm_interface_metadata` is `ReplacingMergeTree` keyed
//!    by `wasm_hash`, so the new row supersedes the old on merge — exactly how
//!    the live writer upserts. No staging/EXCHANGE needed.
//!
//! WASMs whose code entry is archived/expired on RPC are skipped (counted as
//! `missing_on_rpc`) and stay Unknown — safe, never mislabeled.
//!
//! Idempotent: a re-run only re-touches rows still missing the key.
//! `--dry-run` reports the would-be writes without inserting.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use db_clickhouse::persist::rows::WasmInterfaceMetadataRow;
use serde::Deserialize;
use serde_json::Value;
use stellar_xdr::curr::LedgerEntryData;
use tracing::info;

use crate::error::BackfillError;
use crate::rpc_snapshot::{RpcClient, contract_code_ledger_key};
use crate::sink::Sink;

#[derive(Debug, Default, Clone, Copy)]
pub struct UpgradeableBackfillStats {
    /// Rows missing the `upgradeable` key at the start of the pass.
    pub scanned: u64,
    /// WASMs successfully fetched from RPC and parsed.
    pub resolved: u64,
    /// Of `resolved`, how many import the upgrade host fn.
    pub upgradeable: u64,
    /// Of `resolved`, how many are frozen (no import).
    pub frozen: u64,
    /// WASMs not returned by RPC (archived/expired) — left Unknown.
    pub missing_on_rpc: u64,
    pub dry_run: bool,
}

#[derive(Row, Deserialize)]
struct MissingRow {
    /// Hex-encoded (`lower(hex(wasm_hash))`).
    wasm_hash: String,
    metadata: String,
}

pub async fn execute(
    sink: &Sink,
    rpc_url: Option<&str>,
    dry_run: bool,
) -> Result<UpgradeableBackfillStats, BackfillError> {
    let client = match sink {
        Sink::Clickhouse(c) => c,
        Sink::Postgres(_) => {
            // No-op (the flag lives only in the CH metadata JSON). Short-circuit
            // BEFORE the rpc_url check so a Postgres run never needs it.
            info!("upgradeable_backfill: CH-only; Postgres target is a no-op");
            return Ok(UpgradeableBackfillStats {
                dry_run,
                ..Default::default()
            });
        }
    };

    // CH path needs RPC to fetch the WASM bytecode. Required only here, after the
    // Postgres no-op has had its chance to return.
    let rpc_url = rpc_url.ok_or_else(|| {
        BackfillError::Incomplete(
            "ClickHouse upgradeable-backfill requires --soroban-rpc-url (or SOROBAN_RPC_URL)"
                .to_string(),
        )
    })?;

    let missing = read_missing(client).await?;
    let mut stats = UpgradeableBackfillStats {
        scanned: missing.len() as u64,
        dry_run,
        ..Default::default()
    };
    if missing.is_empty() {
        info!("upgradeable_backfill: nothing to do — all rows already carry the key");
        return Ok(stats);
    }

    let rpc = RpcClient::new(rpc_url)?;
    let mut rows_out: Vec<WasmInterfaceMetadataRow> = Vec::with_capacity(missing.len());

    // wasm_hash (raw bytes) -> original metadata JSON, for the merge after fetch.
    let mut keys = Vec::with_capacity(missing.len());
    let mut by_hash: std::collections::HashMap<[u8; 32], String> =
        std::collections::HashMap::with_capacity(missing.len());
    for row in &missing {
        // Hard-fail on bad hex — `lower(hex(wasm_hash))` from CH is always valid
        // 64-char hex, so this can only mean data corruption, never skip it.
        let Some(hash) = decode_hash(&row.wasm_hash) else {
            return Err(BackfillError::Incomplete(format!(
                "non-decodable wasm_hash from ClickHouse: {:?}",
                row.wasm_hash
            )));
        };
        keys.push(contract_code_ledger_key(hash));
        by_hash.insert(hash, row.metadata.clone());
    }

    let records = rpc.get_ledger_entries(&keys).await?;
    let mut seen: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
    for rec in records {
        let LedgerEntryData::ContractCode(cce) = rec.data else {
            continue;
        };
        let hash = cce.hash.0;
        let Some(metadata) = by_hash.get(&hash) else {
            continue;
        };
        let upgradeable = xdr_parser::contract::wasm_imports_upgrade_fn(cce.code.as_slice());
        seen.insert(hash);
        stats.resolved += 1;
        if upgradeable {
            stats.upgradeable += 1;
        } else {
            stats.frozen += 1;
        }
        rows_out.push(WasmInterfaceMetadataRow {
            wasm_hash: hash,
            metadata: merge_upgradeable(metadata, upgradeable),
        });
    }
    let unresolved: Vec<String> = by_hash
        .keys()
        .filter(|h| !seen.contains(*h))
        .map(hex::encode)
        .collect();
    stats.missing_on_rpc = unresolved.len() as u64;

    if dry_run {
        info!(
            scanned = stats.scanned,
            resolved = stats.resolved,
            upgradeable = stats.upgradeable,
            frozen = stats.frozen,
            missing_on_rpc = stats.missing_on_rpc,
            "upgradeable_backfill: dry-run, no rows written"
        );
        return Ok(stats);
    }

    // Write the rows we did resolve first (idempotent — a re-run only retries
    // whatever is still missing the key)…
    let mut insert = client
        .insert::<WasmInterfaceMetadataRow>("wasm_interface_metadata")
        .await?;
    for row in &rows_out {
        insert.write(row).await?;
    }
    insert.end().await?;
    info!(
        written = rows_out.len(),
        "upgradeable_backfill: wrote resolved rows"
    );

    // …then HARD-FAIL if any target WASM could not be resolved, rather than
    // silently leaving it Unknown. These are in-use wasm_hashes (current code of
    // a live contract) so a missing one is a real anomaly (e.g. archived state
    // needing restore) the operator must see. Re-run after fixing; it's idempotent.
    if !unresolved.is_empty() {
        let sample: Vec<&String> = unresolved.iter().take(10).collect();
        return Err(BackfillError::Incomplete(format!(
            "{} of {} in-use WASMs had no ContractCode on RPC (wrote {} resolved). \
             First unresolved: {:?}",
            unresolved.len(),
            by_hash.len(),
            rows_out.len(),
            sample
        )));
    }
    info!("upgradeable_backfill: done — all targets resolved");
    Ok(stats)
}

async fn read_missing(
    client: &ClickhouseClient,
) -> Result<Vec<MissingRow>, clickhouse::error::Error> {
    // Scope to wasm_hashes that are the CURRENT code of some live contract — the
    // only ones a contract page can read. Unused old-version WASMs are skipped:
    // they need no badge and their code entries are the ones most likely archived
    // (which would otherwise trip the hard-fail for no user-visible benefit).
    // Also require a real `functions` key to skip pre-insert stub rows.
    //
    // By design this CANNOT cover a contract whose wasm_hash has NO
    // `wasm_interface_metadata` row at all (never parsed — a handful exist): such
    // a hash is never selected, so it stays Unknown (no chip). That's an ingest/
    // parse coverage gap, not this backfill's job — and Unknown is the honest
    // state for an unparsed WASM, never a wrong badge.
    client
        .query(
            "SELECT lower(hex(w.wasm_hash)) AS wasm_hash, w.metadata \
             FROM wasm_interface_metadata w FINAL \
             WHERE NOT JSONHas(w.metadata, 'upgradeable') \
               AND JSONHas(w.metadata, 'functions') \
               AND w.wasm_hash IN ( \
                   SELECT DISTINCT wasm_hash FROM soroban_contracts \
                   WHERE wasm_hash IS NOT NULL \
               )",
        )
        .fetch_all::<MissingRow>()
        .await
}

fn decode_hash(hex_str: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(hex_str).ok()?;
    bytes.try_into().ok()
}

/// Set `upgradeable` on the stored metadata JSON, preserving `functions` /
/// `wasm_byte_len`. Falls back to a fresh object if the stored value is not an
/// object (should never happen — every live write is `json!({...})`).
fn merge_upgradeable(metadata_json: &str, upgradeable: bool) -> String {
    let mut v = serde_json::from_str::<Value>(metadata_json)
        .unwrap_or_else(|_| Value::Object(Default::default()));
    if !v.is_object() {
        v = Value::Object(Default::default());
    }
    if let Value::Object(map) = &mut v {
        map.insert("upgradeable".to_string(), Value::Bool(upgradeable));
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_existing_keys() {
        let merged = merge_upgradeable(r#"{"functions":[],"wasm_byte_len":256}"#, true);
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["upgradeable"], Value::Bool(true));
        assert_eq!(v["wasm_byte_len"], 256);
        assert!(v["functions"].is_array());
    }

    #[test]
    fn merge_overwrites_and_handles_garbage() {
        // pre-existing false key is overwritten
        let merged = merge_upgradeable(r#"{"upgradeable":false}"#, true);
        assert_eq!(
            serde_json::from_str::<Value>(&merged).unwrap()["upgradeable"],
            Value::Bool(true)
        );
        // non-object input degrades to a fresh object with the key
        let merged = merge_upgradeable("not json", false);
        assert_eq!(
            serde_json::from_str::<Value>(&merged).unwrap()["upgradeable"],
            Value::Bool(false)
        );
    }

    #[test]
    fn decode_hash_roundtrip() {
        let h = [0xabu8; 32];
        assert_eq!(decode_hash(&hex::encode(h)), Some(h));
        assert_eq!(decode_hash("zz"), None);
        assert_eq!(decode_hash("ab"), None); // wrong length
    }
}
