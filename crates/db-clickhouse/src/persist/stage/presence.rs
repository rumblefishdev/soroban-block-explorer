//! Presence-index rows staged once per ledger: `transaction_participants` and
//! the event-derived half of `operation_asset_appearances`. The op-derived
//! asset rows stay in the operation loop of `prepare_with_sac_overrides`,
//! which already walks every operation.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{HashMap, HashSet};

use xdr_parser::types::ExtractedTransaction;

use super::is_strkey_account;
use crate::persist::ids;
use crate::persist::rows::{OperationAssetAppearanceRow, TransactionParticipantRow};

/// `transaction_participants`: one row per (account, transaction). Only
/// `G…` accounts — contracts and muxed forms are not participants.
pub(super) fn participant_rows(
    ledger_sequence: i64,
    transactions: &[ExtractedTransaction],
    participants_per_tx: &HashMap<String, HashSet<String>>,
    tx_id_by_hash: &HashMap<String, i64>,
) -> Vec<TransactionParticipantRow> {
    let mut rows = Vec::new();
    for tx in transactions {
        let Some(set) = participants_per_tx.get(&tx.hash) else {
            continue;
        };
        let Some(&tx_id) = tx_id_by_hash.get(&tx.hash) else {
            continue;
        };
        for key in set {
            if !is_strkey_account(key) {
                continue;
            }
            rows.push(TransactionParticipantRow {
                account_id: ids::account_id(key),
                ledger_sequence,
                transaction_id: tx_id,
            });
        }
    }
    rows
}

/// `operation_asset_appearances`, event-derived (task 0383, K3-4): SAC /
/// bespoke token moves (transfer / mint / burn / clawback) make the moved
/// asset appear in the tx. Same (asset, tx) grain as the op-derived rows; the
/// RMT collapses any overlap. Presence only (model A).
pub(super) fn event_asset_rows(
    ledger_sequence: i64,
    event_assets_per_tx: &HashMap<String, HashSet<i64>>,
    tx_id_by_hash: &HashMap<String, i64>,
) -> Vec<OperationAssetAppearanceRow> {
    let mut rows = Vec::new();
    for (tx_hash, asset_ids) in event_assets_per_tx {
        let Some(&tx_id) = tx_id_by_hash.get(tx_hash) else {
            continue;
        };
        for &asset_id in asset_ids {
            rows.push(OperationAssetAppearanceRow {
                asset_id,
                ledger_sequence,
                transaction_id: tx_id,
            });
        }
    }
    rows
}
