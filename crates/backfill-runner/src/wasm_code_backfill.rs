//! Task 0620 — one-shot fill of `wasm_code` with the bytes of every program
//! uploaded before the indexer started writing them.
//!
//! The indexer stores a program's bytes when it sees the upload. Older
//! programs are known only by hash (`wasm_interface_metadata`,
//! `soroban_contracts.wasm_hash`). This pass reads each missing program from
//! Soroban RPC `getLedgerEntries` (`ContractCode` by hash — archived programs
//! are returned too) and keeps it only when its sha256 equals the hash, so a
//! wrong or tampered answer is never stored.
//!
//! Idempotent: a re-run fetches only hashes still missing. `--dry-run`
//! fetches and verifies without writing.

use std::time::Duration;

use clickhouse::Row;
use db_clickhouse::persist::rows::WasmCodeRow;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use stellar_xdr::{Hash, LedgerEntryData, LedgerKey, LedgerKeyContractCode};
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::rpc_snapshot::{RpcClient, RpcError};
use crate::sink::Sink;
use crate::util::insert_rows;

/// Programs per `getLedgerEntries` call. A program is up to 128 KiB, so 20
/// keep one response under ~3 MB.
const PROGRAMS_PER_CALL: usize = 20;

/// Attempts per call when the RPC answers 429 (rate limit), waiting 5 s,
/// 10 s, 15 s… between them.
const ATTEMPTS: u64 = 6;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WasmCodeBackfillStats {
    /// Known program hashes with no `wasm_code` row.
    pub missing: u64,
    /// Programs the RPC returned.
    pub fetched: u64,
    /// Returned programs whose sha256 differs from their hash (not stored).
    pub hash_mismatch: u64,
    /// Rows written (0 on a dry run).
    pub written: u64,
    pub dry_run: bool,
}

#[derive(Row, Deserialize)]
struct MissingHash {
    wasm_hash: [u8; 32],
}

pub async fn execute(
    sink: &Sink,
    rpc_url: Option<&str>,
    dry_run: bool,
) -> Result<WasmCodeBackfillStats, BackfillError> {
    let client = sink.client();
    let rpc_url = rpc_url.ok_or_else(|| {
        BackfillError::Incomplete(
            "wasm_code_backfill requires --soroban-rpc-url (or SOROBAN_RPC_URL)".to_string(),
        )
    })?;

    let missing: Vec<[u8; 32]> = client
        .query(
            "SELECT wasm_hash FROM ( \
                 SELECT wasm_hash FROM wasm_interface_metadata \
                 UNION DISTINCT \
                 SELECT assumeNotNull(wasm_hash) AS wasm_hash FROM soroban_contracts \
                 WHERE wasm_hash IS NOT NULL \
             ) \
             WHERE wasm_hash NOT IN (SELECT wasm_hash FROM wasm_code) \
             ORDER BY wasm_hash",
        )
        .fetch_all::<MissingHash>()
        .await?
        .into_iter()
        .map(|r| r.wasm_hash)
        .collect();

    let mut stats = WasmCodeBackfillStats {
        missing: missing.len() as u64,
        dry_run,
        ..Default::default()
    };
    info!(
        missing = stats.missing,
        "wasm_code_backfill: programs without bytes"
    );

    let rpc = RpcClient::new(rpc_url)?;
    for chunk in missing.chunks(PROGRAMS_PER_CALL) {
        let keys: Vec<LedgerKey> = chunk
            .iter()
            .map(|hash| LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(*hash) }))
            .collect();

        let mut rows = Vec::with_capacity(chunk.len());
        for record in fetch_with_retry(&rpc, &keys).await? {
            let LedgerEntryData::ContractCode(entry) = record.data else {
                continue;
            };
            stats.fetched += 1;
            let code = entry.code.to_vec();
            if Sha256::digest(&code).as_slice() != entry.hash.0.as_slice() {
                warn!(wasm_hash = %hex::encode(entry.hash.0), "program bytes do not match their hash — skipped");
                stats.hash_mismatch += 1;
                continue;
            }
            rows.push(WasmCodeRow {
                wasm_hash: entry.hash.0,
                code,
            });
        }

        if !dry_run {
            insert_rows(client, "wasm_code", &rows).await?;
            stats.written += rows.len() as u64;
        }
        info!(
            fetched = stats.fetched,
            written = stats.written,
            "wasm_code_backfill: progress"
        );
    }

    Ok(stats)
}

async fn fetch_with_retry(
    rpc: &RpcClient,
    keys: &[LedgerKey],
) -> Result<Vec<crate::rpc_snapshot::LedgerEntryRecord>, BackfillError> {
    let mut attempt = 1;
    loop {
        match rpc.get_ledger_entries(keys).await {
            Err(RpcError::HttpStatus { status: 429, .. }) if attempt < ATTEMPTS => {
                tokio::time::sleep(Duration::from_secs(5 * attempt)).await;
                attempt += 1;
            }
            other => return Ok(other?),
        }
    }
}
