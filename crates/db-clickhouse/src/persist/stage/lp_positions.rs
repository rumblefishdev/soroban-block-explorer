//! `lp_positions` rows staged per ledger: one row per (pool, account), the
//! pool-share trustline's latest state — and `lp_first_deposits`, the ledger of
//! each account's deposits into a pool (task 0468).
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{HashMap, HashSet};

use domain::OperationType;
use xdr_parser::types::{ExtractedLpPosition, ExtractedOperation, ExtractedTransaction};

use super::{OpTyped, decimal7_string_to_i128, decode_hash};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::{LpFirstDepositRow, LpPositionRow};

pub(super) fn lp_position_rows(
    lp_positions: &[ExtractedLpPosition],
) -> Result<Vec<LpPositionRow>, SchemaError> {
    use std::collections::hash_map::Entry;
    let mut lp_dedup: HashMap<([u8; 32], i64), LpPositionRow> = HashMap::new();
    for pos in lp_positions {
        let pool_id = decode_hash(&pos.pool_id, "lp_position.pool_id")?;
        let acct_id = ids::account_id(&pos.account_id);
        let last = i64::from(pos.last_updated_ledger);
        let first = pos.first_deposit_ledger.map(i64::from).unwrap_or(last);
        let new_row = LpPositionRow {
            pool_id,
            account_id: acct_id,
            shares: decimal7_string_to_i128(&pos.shares)?,
            first_deposit_ledger: first,
            last_updated_ledger: last,
            // The pool-share trustline was removed — the participant left the
            // pool, as opposed to withdrawing to zero and staying. ADR 0055.
            closed_at_ledger: if pos.closed { last } else { 0 },
        };
        match lp_dedup.entry((pool_id, acct_id)) {
            Entry::Occupied(mut occ) => {
                let existing = occ.get_mut();
                if new_row.last_updated_ledger >= existing.last_updated_ledger {
                    let preserved_first = existing
                        .first_deposit_ledger
                        .min(new_row.first_deposit_ledger);
                    *existing = new_row;
                    existing.first_deposit_ledger = preserved_first;
                } else {
                    existing.first_deposit_ledger = existing
                        .first_deposit_ledger
                        .min(new_row.first_deposit_ledger);
                }
            }
            Entry::Vacant(vac) => {
                vac.insert(new_row);
            }
        }
    }
    Ok(lp_dedup.into_values().collect())
}

/// One row per (pool, depositor) that deposited in this ledger, for
/// `lp_first_deposits`, which keeps the minimum ledger per key.
///
/// Only successful transactions count: a failed one keeps its operations in
/// `transaction_operations` (18.4% of all deposits), but it moved no reserves
/// and issued no shares. The depositor is the op's source, else the
/// transaction's — 41% of deposits carry no source of their own.
pub(super) fn lp_first_deposit_rows(
    transactions: &[ExtractedTransaction],
    operations: &[(String, Vec<ExtractedOperation>)],
    ledger_sequence: i64,
) -> Result<Vec<LpFirstDepositRow>, SchemaError> {
    let source_of_successful: HashMap<&str, &str> = transactions
        .iter()
        .filter(|tx| tx.successful)
        .map(|tx| (tx.hash.as_str(), tx.source_account.as_str()))
        .collect();
    let mut deposited: HashSet<([u8; 32], i64)> = HashSet::new();
    for (tx_hash, ops) in operations {
        let Some(&tx_source) = source_of_successful.get(tx_hash.as_str()) else {
            continue;
        };
        for op in ops {
            if op.op_type != OperationType::LiquidityPoolDeposit {
                continue;
            }
            let depositor = op.source_account.as_deref().unwrap_or(tx_source);
            for pool in OpTyped::from_details(op.op_type, &op.details).pool_ids_hex {
                let pool_id = decode_hash(&pool, "deposit.liquidityPoolId")?;
                deposited.insert((pool_id, ids::account_id(depositor)));
            }
        }
    }
    Ok(deposited
        .into_iter()
        .map(|(pool_id, account_id)| LpFirstDepositRow {
            pool_id,
            account_id,
            first_deposit_ledger: ledger_sequence,
        })
        .collect())
}

#[cfg(test)]
mod tests;
