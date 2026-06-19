//! ClickHouse writer module — populates the 17 production-schema
//! tables from parser output.
//!
//! Architecture (task 0206, hybrid-surrogate redesign):
//!
//! * [`stage::prepare`] — synchronous pre-write transform. Reads the
//!   same `Extracted*` slices PG does and produces CH-shaped row
//!   structs from [`rows`]. FK columns use a **hybrid key strategy**:
//!   three high-fan-out hubs (`accounts`, `soroban_contracts`,
//!   `transactions`) carry a deterministic surrogate `Int64 id`
//!   derived via [`ids`] (`cityhash_102_128` lower 64 bits over the
//!   StrKey / hash), keeping referencing tables narrow; the other 12
//!   tables use natural composite keys (StrKey / hash / tuple) on
//!   their PK and reference the hubs by surrogate. Surrogates are
//!   derived synchronously from the source key, so the writer is
//!   still single-pass and fully deterministic across replays.
//!   `LowCardinality(String)` dictionary-encodes per-block-repeated
//!   StrKeys for free on the natural-key columns.
//! * [`writer::PartitionWriter`] — streams rows into long-lived
//!   `clickhouse::Insert<RowT>` handles, one per table per backfill
//!   partition. Implements the commit-marker pattern so a partial
//!   run leaves the partition resumable. See module docs for the
//!   "why per-ledger inserts are wrong" rationale.
//! * [`persist_ledger_clickhouse`] — thin wrapper around a one-shot
//!   `PartitionWriter` (open → write_ledger → commit). Kept for
//!   legacy / single-ledger callers (tests, the runner's
//!   `Sink::persist_ledger` fallback). Production backfill drives
//!   `PartitionWriter` directly via the runner's partition-writer
//!   lifecycle.

use std::collections::{HashMap, HashSet};

use clickhouse::Client;
use domain::ContractType;
// Re-export the canonical (domain-owned) cache so `db_clickhouse::persist::
// ClassificationCache` stays a valid path for callers + integration tests that
// don't depend on `domain` directly (task 0283).
pub use domain::ClassificationCache;
use xdr_parser::types::{
    ContractFunction, ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment,
    ExtractedContractInterface, ExtractedEvent, ExtractedInvocation, ExtractedLedger,
    ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft,
    ExtractedNftEvent, ExtractedOperation, ExtractedTransaction,
};
use xdr_parser::{SacOverride, classify_contract_from_wasm_spec};

use crate::SchemaError;

pub mod enrichment;
pub mod ids;
pub mod rows;
pub mod stage;
pub mod writer;

#[cfg(test)]
mod tests_cross;

pub use writer::PartitionWriter;

/// Per-ledger persist entrypoint kept for legacy callers and one-shot
/// use cases (tests, the single-ledger path on the `Sink` enum). Wraps
/// a one-shot [`PartitionWriter`] so the per-ledger semantics are
/// preserved end-to-end (open → write → commit) even though production
/// backfill goes through the partition-writer lifecycle directly.
///
/// Real production callers should drive [`PartitionWriter`] across the
/// whole backfill partition — per-ledger opens defeat the part-economy
/// design (see [`writer`] module docs). This wrapper exists so the
/// 0205-era signature stays callable.
#[allow(clippy::too_many_arguments)]
pub async fn persist_ledger_clickhouse(
    client: &Client,
    ledger: &ExtractedLedger,
    transactions: &[ExtractedTransaction],
    operations: &[(String, Vec<ExtractedOperation>)],
    events: &[(String, Vec<ExtractedEvent>)],
    invocations: &[(String, Vec<ExtractedInvocation>)],
    contract_interfaces: &[ExtractedContractInterface],
    contract_deployments: &[ExtractedContractDeployment],
    account_states: &[ExtractedAccountState],
    liquidity_pools: &[ExtractedLiquidityPool],
    pool_snapshots: &[ExtractedLiquidityPoolSnapshot],
    assets: &[ExtractedAsset],
    nfts: &[ExtractedNft],
    nft_events: &[ExtractedNftEvent],
    lp_positions: &[ExtractedLpPosition],
    contract_name_writes: &[(String, String)],
    contract_metadata_writes: &[xdr_parser::ExtractedContractMetadata],
    sac_overrides: &[SacOverride],
    classification_cache: &ClassificationCache,
) -> Result<(), SchemaError> {
    // Task 0283 live G1: resolve cross-ledger WASM verdicts the pure stage
    // can't see (upload + deploy are separate Soroban txs, almost always in
    // different ledgers). Fail-open + gated to deploy-bearing ledgers — an
    // empty map reproduces the pre-0283 behaviour exactly.
    let prior_wasm_verdicts =
        fetch_prior_wasm_verdicts(client, contract_deployments, contract_interfaces).await;
    // Task 0283 live G9: resolve verdicts for contracts that emit NFT
    // rows/events THIS ledger but were deployed in an EARLIER one. The stage's
    // `route_for` only sees this-ledger `contract_rows`, so without this a
    // later transfer from an already-classified NFT routes to quarantine.
    // Fail-open + gated to event-bearing ledgers; one batched lookup per
    // ledger (Option (b) — a cross-invocation cache can wrap this later).
    let prior_contract_verdicts = fetch_prior_contract_verdicts(
        client,
        nfts,
        nft_events,
        contract_deployments,
        classification_cache,
    )
    .await;
    let mut staged = stage::prepare_with_sac_overrides(
        ledger,
        transactions,
        operations,
        events,
        invocations,
        contract_interfaces,
        contract_deployments,
        account_states,
        liquidity_pools,
        pool_snapshots,
        assets,
        nfts,
        nft_events,
        lp_positions,
        contract_name_writes,
        sac_overrides,
        &prior_wasm_verdicts,
        &prior_contract_verdicts,
    )?;
    // ADR 0049: on-chain token metadata → its own side table, assigned here
    // (not threaded through `prepare`). SAC filtering already done upstream.
    staged.metadata_rows = stage::build_metadata_rows(contract_metadata_writes);
    let mut pw = PartitionWriter::open(client.clone());
    if let Err(err) = pw.write_ledger(staged).await {
        pw.abort().await;
        return Err(err);
    }
    pw.commit().await
}

/// Subset of `wasm_interface_metadata.metadata` we need to re-classify a
/// WASM by its already-persisted function list. The column is the JSON
/// written by [`stage`] as `{"functions": [...], "wasm_byte_len": N}`; we
/// only read `functions` and feed it back through the canonical
/// [`classify_contract_from_wasm_spec`] for exact parity with the deploy-time
/// path (no rule duplication).
#[derive(serde::Deserialize)]
struct WasmMetadata {
    functions: Vec<ContractFunction>,
}

/// One `(hash, metadata)` row read from `wasm_interface_metadata` during the
/// live G1 prefetch. `h` is the lower-case hex of the `FixedString(32)`
/// `wasm_hash` (`lower(hex(...))` server-side) so it matches the hex the
/// parser emits without binding raw bytes.
#[derive(clickhouse::Row, serde::Deserialize)]
struct WasmVerdictRow {
    h: String,
    metadata: String,
}

/// Task 0283 live G1 — pre-fetch cross-ledger WASM verdicts for the deploys
/// in this ledger.
///
/// The pure stage (`stage::prepare`) only sees WASM uploaded in the SAME
/// ledger as a deploy. On Soroban `uploadContractWasm` and `createContract`
/// are separate transactions in (almost always) different ledgers, so a
/// deploy's WASM verdict is invisible to the stage and the contract would
/// persist the parser default `Other` — its NFT events then route to the
/// quarantine and the hot tables stay empty. This reads the verdict for those
/// deploy hashes from the already-persisted `wasm_interface_metadata` (the
/// upload landed in an earlier, already-committed ledger) and hands it to the
/// stage's existing deploy override.
///
/// Gated + fail-open by construction:
/// * No non-SAC deploy carrying a `wasm_hash` (≈99.8% of ledgers) → returns
///   empty with **no** round trip.
/// * Hashes already classified in THIS ledger are dropped from the lookup —
///   the same-ledger map already covers them.
/// * Any query / parse failure → empty map + `warn!`, so the writer behaves
///   exactly as pre-0283 (`Other` → quarantine) and the batch backstop
///   (`nft-reclassify`) drains later. The lookup can only improve a verdict,
///   never block a write.
async fn fetch_prior_wasm_verdicts(
    client: &Client,
    contract_deployments: &[ExtractedContractDeployment],
    contract_interfaces: &[ExtractedContractInterface],
) -> HashMap<[u8; 32], ContractType> {
    let same_ledger: HashSet<String> = contract_interfaces
        .iter()
        .map(|i| i.wasm_hash.to_lowercase())
        .collect();
    let mut want: Vec<String> = contract_deployments
        .iter()
        .filter(|d| !d.is_sac)
        .filter_map(|d| d.wasm_hash.as_deref())
        .map(str::to_lowercase)
        .filter(|h| !same_ledger.contains(h))
        .collect();
    want.sort();
    want.dedup();
    if want.is_empty() {
        return HashMap::new();
    }

    match query_wasm_verdicts(client, &want).await {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "0283 live G1: wasm verdict prefetch failed — deploys persist as Other, batch backstop will drain"
            );
            HashMap::new()
        }
    }
}

/// Read + classify the verdicts for `hashes_hex_lower` from
/// `wasm_interface_metadata`. Returns only `Nft` / `Fungible` verdicts — the
/// deploy override ignores anything else, and `Other` is already the parser
/// default, so there is nothing to override.
async fn query_wasm_verdicts(
    client: &Client,
    hashes_hex_lower: &[String],
) -> Result<HashMap<[u8; 32], ContractType>, clickhouse::error::Error> {
    // The hashes are 64-char lower-case hex derived from parser output — no
    // injection surface. Compared against `lower(hex(wasm_hash))` because CH
    // `hex()` upper-cases while the parser / `hex::encode` produce lower-case.
    let in_list = hashes_hex_lower
        .iter()
        .map(|h| format!("'{h}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT lower(hex(wasm_hash)) AS h, metadata \
         FROM wasm_interface_metadata \
         WHERE lower(hex(wasm_hash)) IN ({in_list})"
    );

    let rows = client.query(&sql).fetch_all::<WasmVerdictRow>().await?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let Ok(bytes) = hex::decode(&row.h) else {
            continue;
        };
        let Ok(hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<WasmMetadata>(&row.metadata) else {
            continue;
        };
        let verdict: ContractType = classify_contract_from_wasm_spec(&meta.functions).into();
        if matches!(verdict, ContractType::Nft | ContractType::Fungible) {
            out.insert(hash, verdict);
        }
    }
    Ok(out)
}

/// One `(contract_id, contract_type)` row for the live G9 verdict lookup.
#[derive(clickhouse::Row, serde::Deserialize)]
struct ContractVerdictRow {
    contract_id: String,
    contract_type: i16,
}

/// Task 0283 live G9 — resolve cross-ledger verdicts for contracts that emit
/// NFT rows/events in this ledger but were deployed earlier.
///
/// `route_for` (stage) builds its verdict map only from THIS ledger's
/// `contract_rows`, so a transfer from a contract deployed in a prior ledger
/// has no verdict and routes to the quarantine. This reads the already-known
/// verdict straight from `soroban_contracts` (populated at deploy time by G1)
/// and hands it to `route_for` as a fallback, so the event routes correctly
/// (Nft → hot, SAC/Fungible → drop) immediately.
///
/// Gated + fail-open:
/// * No NFT row/event from a not-deployed-here contract → empty, **no** round
///   trip. Contracts deployed THIS ledger are excluded — `contract_rows`
///   already covers them.
/// * Any query failure → empty map + `warn!`; routing falls back to
///   quarantine exactly as pre-G9 and the one-shot `nft-reclassify` drain (or
///   a re-run) recovers it. Can only improve routing, never block a write.
///
/// Option (c): a lazy per-(warm-container) [`ClassificationCache`] (the PG
/// `ClassificationCache` pattern) memoizes definitive verdicts, so the batched
/// DB lookup fires only on the FIRST sighting of a contract (~9/day steady
/// state) rather than once per event-bearing ledger (~80% of ledgers).
async fn fetch_prior_contract_verdicts(
    client: &Client,
    nfts: &[ExtractedNft],
    nft_events: &[ExtractedNftEvent],
    contract_deployments: &[ExtractedContractDeployment],
    cache: &ClassificationCache,
) -> HashMap<String, ContractType> {
    let deployed_here: HashSet<&str> = contract_deployments
        .iter()
        .map(|d| d.contract_id.as_str())
        .collect();
    let mut want: Vec<String> = nfts
        .iter()
        .map(|n| n.contract_id.clone())
        .chain(nft_events.iter().map(|e| e.contract_id.clone()))
        .filter(|c| !deployed_here.contains(c.as_str()))
        .collect();
    want.sort();
    want.dedup();
    if want.is_empty() {
        return HashMap::new();
    }

    // Only the contracts not yet memoized hit the DB; `Other`/unknown are never
    // cached, so they re-resolve until they earn a definitive verdict.
    let missing = cache.missing(want.iter().map(String::as_str));
    if !missing.is_empty() {
        match query_contract_verdicts(client, &missing).await {
            Ok(fetched) => cache.extend_definitive(fetched),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "0283 live G9: contract verdict prefetch failed — events route to quarantine, drain recovers"
                );
                // Fail-open: don't populate; uncached contracts route to pending.
            }
        }
    }
    // `snapshot_for` borrows from `want`; copy to owned for the stage map.
    cache
        .snapshot_for(want.iter().map(String::as_str))
        .into_iter()
        .map(|(id, ty)| (id.to_string(), ty))
        .collect()
}

/// Read decisive verdicts (`Token` / `Nft` / `Fungible`; `Other`/NULL skipped
/// — they route to quarantine anyway) for `contract_ids` from
/// `soroban_contracts`. Including `Token` lets SAC-emitted events DROP at
/// routing instead of leaking into pending (closes the 0221 write-time leak).
async fn query_contract_verdicts(
    client: &Client,
    contract_ids: &[&str],
) -> Result<HashMap<String, ContractType>, clickhouse::error::Error> {
    // `contract_ids` are `C…` StrKeys from parser output (base32, no quote
    // chars) — no injection surface.
    let in_list = contract_ids
        .iter()
        .map(|c| format!("'{c}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT contract_id, contract_type FROM soroban_contracts FINAL \
         WHERE contract_id IN ({in_list}) AND contract_type IN (0, 2, 3)"
    );

    let rows = client.query(&sql).fetch_all::<ContractVerdictRow>().await?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        if let Ok(verdict) = ContractType::try_from(row.contract_type) {
            out.insert(row.contract_id, verdict);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_ledger() -> ExtractedLedger {
        ExtractedLedger {
            sequence: 1,
            hash: "00".repeat(32),
            closed_at: 0,
            protocol_version: 22,
            transaction_count: 0,
            base_fee: 100,
        }
    }

    /// `persist_ledger_clickhouse` against an unroutable URL must
    /// surface a transport error — we treat it as the "no side effect"
    /// proof that the wrapper actually issues the insert path
    /// (the stub it replaced returned Ok without any I/O).
    #[tokio::test]
    async fn wrapper_returns_err_when_client_unreachable() {
        // Deliberately unroutable URL — any real network use must fail.
        let client = Client::default().with_url("http://127.0.0.1:1");
        let ledger = synthetic_ledger();
        // Single ledger, no transactions or downstream data — the
        // writer still opens the `ledgers` insert at commit time, which
        // touches the network and surfaces the unroutable-URL error.
        let res = persist_ledger_clickhouse(
            &client,
            &ledger,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &ClassificationCache::new(),
        )
        .await;
        assert!(res.is_err(), "expected transport error, got: {res:?}");
    }
}
