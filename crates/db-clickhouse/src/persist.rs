//! ClickHouse writer — currently a stub.
//!
//! Task 0205 wires the runner plumbing end-to-end against a no-op
//! persist call so the parse half can be exercised against
//! `--target clickhouse`. Real INSERTs into the 17 pilot tables land
//! in a follow-up task gated on this stub-driven path being green.
//!
//! The function signature mirrors
//! `indexer::handler::persist::persist_ledger` minus the
//! `classification_cache` parameter (PG-specific NFT filter helper,
//! task 0118 Phase 2). The CH stub does not classify; re-introduce
//! when real INSERTs need it.

use clickhouse::Client;
use xdr_parser::types::{
    ExtractedAccountState, ExtractedAsset, ExtractedContractDeployment, ExtractedContractInterface,
    ExtractedEvent, ExtractedInvocation, ExtractedLedger, ExtractedLiquidityPool,
    ExtractedLiquidityPoolSnapshot, ExtractedLpPosition, ExtractedNft, ExtractedNftEvent,
    ExtractedOperation, ExtractedTransaction,
};

use crate::SchemaError;

/// Stub ClickHouse persist — logs per-ledger context and returns `Ok`.
///
/// No writes are issued. The `client` is held in the signature so the
/// follow-up that lands real INSERTs does not have to change every
/// call site again.
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
    tracing::info!(
        ledger_sequence = ledger.sequence,
        tx_count = transactions.len(),
        ops = operations.iter().map(|(_, v)| v.len()).sum::<usize>(),
        events = events.iter().map(|(_, v)| v.len()).sum::<usize>(),
        invocations = invocations.iter().map(|(_, v)| v.len()).sum::<usize>(),
        contract_interfaces = contract_interfaces.len(),
        contract_deployments = contract_deployments.len(),
        account_states = account_states.len(),
        liquidity_pools = liquidity_pools.len(),
        pool_snapshots = pool_snapshots.len(),
        assets = assets.len(),
        nfts = nfts.len(),
        nft_events = nft_events.len(),
        lp_positions = lp_positions.len(),
        contract_name_writes = contract_name_writes.len(),
        "persist_ledger_clickhouse called (stub — no writes)"
    );
    // TODO(follow-up): implement actual INSERTs into the 17 CH tables.
    let _ = client;
    Ok(())
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

    /// Stub must return `Ok(())` and not touch the client. We can't
    /// directly assert "no HTTP call" without a mocked transport, but
    /// constructing a `Client` pointing at an unreachable URL and
    /// observing `Ok(())` is sufficient: a real INSERT would error.
    #[tokio::test]
    async fn stub_returns_ok_without_touching_client() {
        // Deliberately unroutable URL — any real network use would fail.
        let client = Client::default().with_url("http://127.0.0.1:1");
        let ledger = synthetic_ledger();
        let result = persist_ledger_clickhouse(
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
        assert!(result.is_ok(), "stub should be a no-op: {result:?}");
    }
}
