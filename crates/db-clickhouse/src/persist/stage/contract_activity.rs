//! `contract_activity` rows staged per ledger (task 0586): which contracts
//! each transaction touched, located by the transaction position, with the
//! caller and the call count when the contract was invoked.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{BTreeSet, HashMap};

use xdr_parser::types::ExtractedInvocation;

use super::{StagedLedger, is_strkey_account};
use crate::persist::ids;
use crate::persist::rows::ContractActivityRow;

/// `contract_txs` arrives holding the (contract, position) of every operation
/// event; invocations and operations naming a contract are added here.
pub(super) fn rows(
    out: &mut StagedLedger,
    invocations: &[(String, Vec<ExtractedInvocation>)],
    app_order_by_hash: &HashMap<String, i16>,
    mut contract_txs: BTreeSet<(i64, i16)>,
    ledger_sequence_i64: i64,
) {
    // ---- the invocation fold (ADR 0034): per (contract, transaction), how
    // many times it was called and the first call's caller ----
    struct Invoked {
        count: i32,
        caller_id: Option<i64>,
        caller_contract_id: Option<i64>,
    }
    let mut invoked: HashMap<(i64, i16), Invoked> = HashMap::new();
    for (tx_hash, invs) in invocations {
        let Some(&position) = app_order_by_hash.get(tx_hash) else {
            continue;
        };
        for inv in invs {
            let Some(contract) = &inv.contract_id else {
                continue;
            };
            let (caller_id, caller_contract_id) = match inv.caller_account.as_deref() {
                Some(k) if is_strkey_account(k) => (Some(ids::account_id(k)), None),
                Some(k) if k.starts_with('C') => (None, Some(ids::contract_id(k))),
                _ => (None, None),
            };
            invoked
                .entry((ids::contract_id(contract), position))
                .and_modify(|i| {
                    i.count = i.count.saturating_add(1);
                    if i.caller_id.is_none() && i.caller_contract_id.is_none() {
                        i.caller_id = caller_id;
                        i.caller_contract_id = caller_contract_id;
                    }
                })
                .or_insert(Invoked {
                    count: 1,
                    caller_id,
                    caller_contract_id,
                });
        }
    }

    // ---- which contracts each transaction touched ----
    //
    // The union of the three ways a transaction touches a contract — an
    // operation event it emits, an invocation, an operation naming it — which
    // are exactly the sources the per-contract transaction list reads (task
    // 0541). Fee events are left out: every transaction pays one to the native
    // SAC, which would make that contract's list every transaction on the
    // network. A pair missing from `invoked` was touched, not invoked.
    contract_txs.extend(invoked.keys().copied());
    for op in &out.tx_operation_rows {
        if let Some(contract_id) = op.contract_id {
            contract_txs.insert((contract_id, op.application_order));
        }
    }
    out.contract_activity_rows = contract_txs
        .into_iter()
        .map(|(contract_id, application_order)| {
            let inv = invoked.get(&(contract_id, application_order));
            ContractActivityRow {
                contract_id,
                ledger_sequence: ledger_sequence_i64,
                application_order,
                caller_id: inv.and_then(|i| i.caller_id),
                caller_contract_id: inv.and_then(|i| i.caller_contract_id),
                invocation_count: inv.map_or(0, |i| i.count),
            }
        })
        .collect();
}

#[cfg(test)]
mod tests;
