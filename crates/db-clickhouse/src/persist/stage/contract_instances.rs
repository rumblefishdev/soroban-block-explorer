//! `contract_instances` rows: each contract's instance entry as of this
//! ledger, kept whole for running the contract's own functions (task 0620).
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use xdr_parser::contract_instance::ExtractedContractInstance;

use crate::persist::rows::ContractInstanceRow;

/// One row per contract: the last instance this ledger left it with.
///
/// Folded because an instance can change in several transactions of one
/// ledger, and every write carries the same `ledger` version — the
/// ReplacingMergeTree would keep whichever row it inserted last. `instances`
/// is in application order, so the last one is the value the ledger ended on.
pub(super) fn contract_instance_rows(
    instances: &[ExtractedContractInstance],
) -> Vec<ContractInstanceRow> {
    let rows = instances
        .iter()
        .map(|i| ContractInstanceRow {
            contract: i.contract,
            data_xdr: i.data_xdr.clone(),
            ledger: i64::from(i.ledger_sequence),
        })
        .collect();
    xdr_parser::fold::keep_last_by_key(rows, |r: &ContractInstanceRow| (r.contract, r.ledger))
}

#[cfg(test)]
mod tests;
