//! Persistence layer: writes all parsed data within a single DB transaction.

use std::collections::HashMap;
use tracing::warn;

use super::HandlerError;
use super::convert;
use xdr_parser::types::{
    ExtractedAccountState, ExtractedContractDeployment, ExtractedContractInterface, ExtractedEvent,
    ExtractedInvocation, ExtractedLedger, ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot,
    ExtractedNft, ExtractedOperation, ExtractedToken, ExtractedTransaction,
};

/// Persist all parsed data for a single ledger within `db_tx`.
///
/// The caller is responsible for calling `db_tx.commit()` on success.
#[allow(clippy::too_many_arguments)]
pub async fn persist_ledger(
    db_tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ledger: &ExtractedLedger,
    transactions: &[ExtractedTransaction],
    operations: &[(String, Vec<ExtractedOperation>)],
    events: &[(String, Vec<ExtractedEvent>)],
    invocations: &[(String, Vec<ExtractedInvocation>)],
    operation_trees: &[(String, serde_json::Value)],
    contract_interfaces: &[ExtractedContractInterface],
    contract_deployments: &[ExtractedContractDeployment],
    account_states: &[ExtractedAccountState],
    liquidity_pools: &[ExtractedLiquidityPool],
    pool_snapshots: &[ExtractedLiquidityPoolSnapshot],
    tokens: &[ExtractedToken],
    nfts: &[ExtractedNft],
) -> Result<(), HandlerError> {
    // 1. Insert ledger
    let domain_ledger = convert::to_ledger(ledger);
    db::persistence::insert_ledger(&mut **db_tx, &domain_ledger).await?;

    // 2. Insert transactions and collect hash→id mapping
    let domain_txs: Vec<_> = transactions.iter().map(convert::to_transaction).collect();
    let tx_ids = db::persistence::insert_transactions_batch(&mut **db_tx, &domain_txs).await?;

    let hash_to_id: HashMap<&str, i64> = tx_ids.iter().map(|(h, id)| (h.as_str(), *id)).collect();
    let hash_to_source: HashMap<&str, &str> = transactions
        .iter()
        .map(|t| (t.hash.as_str(), t.source_account.as_str()))
        .collect();

    // 3. Insert operations (resolve transaction_hash → transaction_id)
    for (tx_hash, ops) in operations {
        let Some(&tx_id) = hash_to_id.get(tx_hash.as_str()) else {
            warn!(tx_hash, "no transaction_id found for operations — skipping");
            continue;
        };
        let tx_source = hash_to_source.get(tx_hash.as_str()).copied().unwrap_or("");
        let domain_ops: Vec<_> = ops
            .iter()
            .map(|op| convert::to_operation(op, tx_id, tx_source))
            .collect();
        db::persistence::insert_operations_batch(&mut **db_tx, &domain_ops).await?;
    }

    // 3b. Ensure all referenced contracts exist (FK constraint on soroban_events/invocations)
    let mut contract_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (_tx_hash, evts) in events {
        for e in evts {
            if let Some(ref cid) = e.contract_id {
                contract_ids.insert(cid.as_str());
            }
        }
    }
    for (_tx_hash, invs) in invocations {
        for inv in invs {
            if let Some(ref cid) = inv.contract_id {
                contract_ids.insert(cid.as_str());
            }
        }
    }
    for cid in &contract_ids {
        db::soroban::ensure_contract_exists(&mut **db_tx, cid).await?;
    }

    // 4. Insert events
    for (tx_hash, evts) in events {
        let Some(&tx_id) = hash_to_id.get(tx_hash.as_str()) else {
            warn!(tx_hash, "no transaction_id found for events — skipping");
            continue;
        };
        let domain_events: Vec<_> = evts.iter().map(|e| convert::to_event(e, tx_id)).collect();
        db::persistence::insert_events_batch(&mut **db_tx, &domain_events).await?;
    }

    // 5. Insert invocations
    for (tx_hash, invs) in invocations {
        let Some(&tx_id) = hash_to_id.get(tx_hash.as_str()) else {
            warn!(
                tx_hash,
                "no transaction_id found for invocations — skipping"
            );
            continue;
        };
        let domain_invs: Vec<_> = invs
            .iter()
            .map(|inv| convert::to_invocation(inv, tx_id))
            .collect();
        db::persistence::insert_invocations_batch(&mut **db_tx, &domain_invs).await?;
    }

    // 6. Update operation trees
    for (tx_hash, tree) in operation_trees {
        let Some(&tx_id) = hash_to_id.get(tx_hash.as_str()) else {
            warn!(
                tx_hash,
                "no transaction_id found for operation_tree — skipping"
            );
            continue;
        };
        db::soroban::update_operation_tree(&mut **db_tx, tx_id, tree).await?;
    }

    // 7. Upsert contract deployments (before interface metadata so wasm_hash is populated)
    for deployment in contract_deployments {
        let domain_contract = convert::to_contract(deployment);
        db::soroban::upsert_contract_deployment(&mut **db_tx, &domain_contract).await?;
    }

    // 8. Contract interface metadata — dual-path persistence for the 2-ledger deploy pattern.
    //
    // Soroban separates WASM upload (ContractCodeEntry, ledger A) from contract deployment
    // (ContractDataEntry, ledger B). ExtractedContractInterface is only produced from
    // ContractCodeEntry, so by ledger B there is no interface data to apply directly.
    //
    // Strategy:
    //   a) Always upsert into wasm_interface_metadata (staging by wasm_hash) — covers ledger B.
    //   b) Also apply directly to any soroban_contracts rows that already exist — covers
    //      same-ledger deploys and re-index flows.
    //
    // upsert_contract_deployment() reads wasm_interface_metadata after each deployment upsert,
    // so any contract deployed in a later ledger automatically picks up the staged metadata.
    for iface in contract_interfaces {
        let metadata = serde_json::json!({
            "functions": iface.functions,
            "wasm_byte_len": iface.wasm_byte_len,
        });
        db::soroban::upsert_wasm_interface_metadata(&mut **db_tx, &iface.wasm_hash, &metadata)
            .await?;
        db::soroban::update_contract_interfaces_by_wasm_hash(
            &mut **db_tx,
            &iface.wasm_hash,
            &metadata,
        )
        .await?;
    }

    // 9. Upsert account states
    for account in account_states {
        let domain_account = convert::to_account(account);
        db::soroban::upsert_account_state(&mut **db_tx, &domain_account).await?;
    }

    // 10. Upsert liquidity pools
    for lp in liquidity_pools {
        let domain_pool = convert::to_liquidity_pool(lp);
        db::soroban::upsert_liquidity_pool(&mut **db_tx, &domain_pool).await?;
    }

    // 11. Insert pool snapshots
    for snapshot in pool_snapshots {
        let domain_snapshot = convert::to_pool_snapshot(snapshot);
        db::soroban::insert_liquidity_pool_snapshot(&mut **db_tx, &domain_snapshot).await?;
    }

    // 12. Upsert tokens
    for token in tokens {
        let domain_token = convert::to_token(token);
        db::soroban::upsert_token(&mut **db_tx, &domain_token).await?;
    }

    // 13. Upsert NFTs
    for nft in nfts {
        let domain_nft = convert::to_nft(nft);
        db::soroban::upsert_nft(&mut **db_tx, &domain_nft).await?;
    }

    Ok(())
}
