//! ClickHouse writer module — populates the 17 production-schema
//! tables from parser output.
//!
//! Architecture (task 0206, post-natural-key redesign):
//!
//! * [`stage::prepare`] — synchronous pre-write transform. Reads the
//!   same `Extracted*` slices PG does and produces CH-shaped row
//!   structs from [`rows`]. All FK columns carry **natural keys**
//!   (StrKey, hash, tuple) — no surrogate IDs, no `cityhash`
//!   derivation. `LowCardinality(String)` dictionary-encodes
//!   per-block-repeated StrKeys for free.
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

use clickhouse::Client;
use xdr_parser::types::{
    ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment, ExtractedContractInterface,
    ExtractedEvent, ExtractedInvocation, ExtractedLedger, ExtractedLiquidityPool,
    ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft, ExtractedNftEvent,
    ExtractedOperation, ExtractedTransaction,
};

use crate::SchemaError;

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
) -> Result<(), SchemaError> {
    let staged = stage::prepare(
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
    )?;
    let mut pw = PartitionWriter::open(client.clone());
    if let Err(err) = pw.write_ledger(staged).await {
        pw.abort().await;
        return Err(err);
    }
    pw.commit().await
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
        )
        .await;
        assert!(res.is_err(), "expected transport error, got: {res:?}");
    }
}
