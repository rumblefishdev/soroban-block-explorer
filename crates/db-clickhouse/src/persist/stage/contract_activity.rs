//! Contract-derived rows staged per ledger: `soroban_invocations_appearances`
//! (the invocation fold) and `contract_transactions` (which contracts each
//! transaction touched).
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{BTreeSet, HashMap};

use xdr_parser::types::ExtractedInvocation;

use super::{StagedLedger, is_strkey_account, staging_err};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::{ContractTransactionRow, SorobanInvocationAppearanceRow};

/// `contract_txs` arrives holding the (contract, position) of every operation
/// event; invocations and operations naming a contract are added here.
pub(super) fn contract_rows(
    out: &mut StagedLedger,
    invocations: &[(String, Vec<ExtractedInvocation>)],
    tx_id_by_hash: &HashMap<String, i64>,
    mut contract_txs: BTreeSet<(i64, i16)>,
    ledger_sequence_i64: i64,
) -> Result<(), SchemaError> {
    // ---- soroban_invocations_appearances (ADR 0034 fold) ----
    #[derive(Eq, PartialEq, Hash)]
    struct InvKey {
        contract_strkey: String,
        tx_hash_hex: String,
    }
    struct InvAgg {
        amount: i32,
        caller_account: Option<String>,
        caller_contract_strkey: Option<String>,
    }
    let mut inv_agg: HashMap<InvKey, InvAgg> = HashMap::new();
    for (tx_hash, invs) in invocations {
        if !tx_id_by_hash.contains_key(tx_hash) {
            continue;
        }
        for inv in invs {
            let Some(contract) = &inv.contract_id else {
                continue;
            };
            let (caller_account, caller_contract) = match inv.caller_account.as_deref() {
                Some(k) if is_strkey_account(k) => (Some(k.to_string()), None),
                Some(k) if k.starts_with('C') => (None, Some(k.to_string())),
                _ => (None, None),
            };
            let key = InvKey {
                contract_strkey: contract.clone(),
                tx_hash_hex: tx_hash.clone(),
            };
            inv_agg
                .entry(key)
                .and_modify(|agg| {
                    agg.amount = agg.amount.saturating_add(1);
                    if agg.caller_account.is_none() && agg.caller_contract_strkey.is_none() {
                        agg.caller_account = caller_account.clone();
                        agg.caller_contract_strkey = caller_contract.clone();
                    }
                })
                .or_insert(InvAgg {
                    amount: 1,
                    caller_account,
                    caller_contract_strkey: caller_contract,
                });
        }
    }
    for (k, agg) in inv_agg {
        let Some(&tx_id) = tx_id_by_hash.get(&k.tx_hash_hex) else {
            continue;
        };
        out.invocation_rows.push(SorobanInvocationAppearanceRow {
            contract_id: ids::contract_id(&k.contract_strkey),
            transaction_id: tx_id,
            ledger_sequence: ledger_sequence_i64,
            caller_id: agg.caller_account.as_deref().map(ids::account_id),
            caller_contract_id: agg.caller_contract_strkey.as_deref().map(ids::contract_id),
            amount: agg.amount,
        });
    }

    // ---- contract_transactions: which contracts each transaction touched ----
    //
    // The union of the three ways a transaction touches a contract — an
    // operation event it emits, an invocation, an operation naming it — which
    // are exactly the sources the per-contract transaction list reads (task
    // 0541). Fee events are left out: every transaction pays one to the native
    // SAC, which would make that contract's list every transaction on the
    // network. Invocations name the transaction by its hash surrogate; the
    // index keys it by position, as the events and operations already do.
    let app_order_by_tx_id: HashMap<i64, i16> = out
        .transaction_rows
        .iter()
        .map(|t| (t.id, t.application_order))
        .collect();
    let position_of = |tx_id: i64| {
        app_order_by_tx_id
            .get(&tx_id)
            .copied()
            .ok_or_else(|| staging_err(&format!("transaction id {tx_id} is not in this ledger")))
    };
    for inv in &out.invocation_rows {
        contract_txs.insert((inv.contract_id, position_of(inv.transaction_id)?));
    }
    for op in &out.tx_operation_rows {
        if let Some(contract_id) = op.contract_id {
            contract_txs.insert((contract_id, op.application_order));
        }
    }
    out.contract_tx_rows = contract_txs
        .into_iter()
        .map(|(contract_id, application_order)| ContractTransactionRow {
            contract_id,
            ledger_sequence: ledger_sequence_i64,
            application_order,
        })
        .collect();
    Ok(())
}
