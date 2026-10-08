//! Contract instances, kept whole for running a contract's own functions
//! (task 0620, ADR 0061).
//!
//! A contract's instance entry holds its executable and its instance storage
//! — where most tokens keep `decimals`, `name` and `symbol`. Running the
//! contract needs the entry exactly as the network stores it, so it is kept as
//! the XDR of its `LedgerEntryData`, not decoded into fields.

use stellar_xdr::*;

/// One contract's instance entry as of a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedContractInstance {
    /// The contract's 32-byte id.
    pub contract: [u8; 32],
    /// XDR of the entry's `LedgerEntryData` (a `ContractData` whose key is
    /// the contract instance).
    pub data_xdr: Vec<u8>,
    pub ledger_sequence: u32,
}

/// Every contract instance this transaction created, changed or brought back
/// from the archive, in change order. `State` is the value before a change and
/// `Removed` carries no value; an instance is never removed by a transaction.
pub fn extract_contract_instances(
    tx_meta: &TransactionMeta,
    ledger_sequence: u32,
) -> Vec<ExtractedContractInstance> {
    let mut instances = Vec::new();
    for change in crate::meta::ledger_changes(tx_meta) {
        let entry = match change {
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::Restored(e) => e,
            LedgerEntryChange::State(_) | LedgerEntryChange::Removed(_) => continue,
        };
        let LedgerEntryData::ContractData(data) = &entry.data else {
            continue;
        };
        if data.key != ScVal::LedgerKeyContractInstance {
            continue;
        }
        let ScAddress::Contract(ContractId(Hash(contract))) = data.contract else {
            continue;
        };
        let data_xdr = match entry.data.to_xdr(Limits::none()) {
            Ok(bytes) => bytes,
            Err(e) => {
                // A value just decoded from the ledger re-encodes; seeing this
                // means the XDR library changed under us.
                tracing::warn!(ledger_sequence, "contract instance does not re-encode: {e}");
                continue;
            }
        };
        instances.push(ExtractedContractInstance {
            contract,
            data_xdr,
            ledger_sequence,
        });
    }
    instances
}

#[cfg(test)]
mod tests;
