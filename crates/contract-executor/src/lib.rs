//! Run a Soroban contract's read-only function locally (task 0620, ADR 0061).
//!
//! A token's `decimals`, `name` and `symbol` are whatever its own functions
//! return, wherever the author chose to keep them. This crate runs such a
//! function with the network's own host library (`soroban-env-host`, the code
//! validators run) over ledger entries the caller supplies: the program's
//! bytes, the contract's instance, and anything else the run turns out to
//! need. Nothing here talks to the network.
//!
//! The run is in recording mode: the host records which entries it reads
//! instead of requiring them up front. An entry the caller did not supply is
//! reported back as missing, so the caller can add it and run again — or give
//! up when it has no source for it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use soroban_env_host::budget::Budget;
use soroban_env_host::e2e_invoke::{
    RecordingInvocationAuthMode, invoke_host_function_in_recording_mode,
};
use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::xdr::{
    AccountId, ConfigSettingEntry, ContractCostParams, ContractId, Hash, HostFunction,
    InvokeContractArgs, LedgerEntry, LedgerEntryData, LedgerKey, Limits, PublicKey, ReadXdr,
    ScAddress, ScSymbol, ScVal, Uint256,
};
use soroban_env_host::{HostError, LedgerInfo};

mod network;

/// The ledger the function runs "at".
#[derive(Debug, Clone, Copy)]
pub struct Ledger {
    pub sequence: u32,
    /// Close time, Unix seconds.
    pub timestamp: u64,
    pub protocol_version: u32,
    /// `sha256` of the network passphrase.
    pub network_id: [u8; 32],
}

/// What one run of a function came to.
#[derive(Debug, PartialEq)]
pub enum ViewOutcome {
    /// The function returned this value.
    Value(ScVal),
    /// The run asked for entries the caller did not supply. Add them and run
    /// again; the value of this run means nothing.
    Missing(Vec<LedgerKey>),
    /// The function failed with every entry it asked for present — the same
    /// failure the network would report.
    Failed(String),
}

/// Run `function` (no arguments) of `contract` over `entries`.
///
/// `entries` maps a ledger key to its entry, or to `None` for a key the
/// caller knows does not exist on the ledger (a contract may test for an
/// optional entry). A key absent from the map is unknown and is reported as
/// missing. Every entry is treated as live: the host requires an expiry for
/// each one, and for a read-only call an archived entry reads exactly like a
/// live one.
pub fn call_view(
    entries: Rc<BTreeMap<LedgerKey, Option<LedgerEntry>>>,
    ledger: &Ledger,
    contract: [u8; 32],
    function: &str,
) -> ViewOutcome {
    let Ok(function_name) = function.try_into().map(ScSymbol) else {
        return ViewOutcome::Failed(format!("`{function}` is not a valid function name"));
    };
    let host_function = HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: ScAddress::Contract(ContractId(Hash(contract))),
        function_name,
        args: Default::default(),
    });
    let snapshot = Rc::new(Snapshot {
        entries,
        live_until: ledger.sequence.saturating_add(network::MIN_PERSISTENT_TTL),
        missing: RefCell::new(Vec::new()),
    });
    let budget = match budget() {
        Ok(budget) => budget,
        Err(e) => return ViewOutcome::Failed(format!("budget: {e:?}")),
    };
    // A read-only call has no signer; the source account is never read.
    let source = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32])));
    let ledger_info = LedgerInfo {
        protocol_version: ledger.protocol_version,
        sequence_number: ledger.sequence,
        timestamp: ledger.timestamp,
        network_id: ledger.network_id,
        base_reserve: network::BASE_RESERVE,
        min_temp_entry_ttl: network::MIN_TEMPORARY_TTL,
        min_persistent_entry_ttl: network::MIN_PERSISTENT_TTL,
        max_entry_ttl: network::MAX_ENTRY_TTL,
    };

    let result = invoke_host_function_in_recording_mode(
        &budget,
        false,
        &host_function,
        &source,
        RecordingInvocationAuthMode::recording(true, false),
        ledger_info,
        snapshot.clone(),
        [0; 32],
        &mut Vec::new(),
    );

    // The host may ask for the same key more than once in a run.
    let mut missing = snapshot.missing.take();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return ViewOutcome::Missing(missing);
    }
    match result {
        Ok(run) => match run.invoke_result {
            Ok(value) => ViewOutcome::Value(value),
            Err(e) => ViewOutcome::Failed(format!("{e:?}")),
        },
        Err(e) => ViewOutcome::Failed(format!("{e:?}")),
    }
}

/// The network's own limits and cost model for one transaction.
fn budget() -> Result<Budget, HostError> {
    Budget::try_from_configs(
        network::TX_MAX_INSTRUCTIONS,
        network::TX_MEMORY_LIMIT,
        cost_params(network::COST_PARAMS_CPU_XDR),
        cost_params(network::COST_PARAMS_MEM_XDR),
    )
}

/// Decode a config-setting entry holding cost parameters. The inputs are the
/// constants in `network.rs`, so a failure is a broken build, not bad data.
fn cost_params(base64: &str) -> ContractCostParams {
    let data = LedgerEntryData::from_xdr_base64(base64.trim(), Limits::none())
        .expect("network cost params decode");
    match data {
        LedgerEntryData::ConfigSetting(ConfigSettingEntry::ContractCostParamsCpuInstructions(
            p,
        ))
        | LedgerEntryData::ConfigSetting(ConfigSettingEntry::ContractCostParamsMemoryBytes(p)) => p,
        other => panic!("not a cost-params config setting: {other:?}"),
    }
}

/// The caller's entries as the host sees them; every key asked for and not
/// supplied is recorded.
struct Snapshot {
    entries: Rc<BTreeMap<LedgerKey, Option<LedgerEntry>>>,
    live_until: u32,
    missing: RefCell<Vec<LedgerKey>>,
}

impl SnapshotSource for Snapshot {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        match self.entries.get(key.as_ref()) {
            Some(Some(entry)) => Ok(Some((Rc::new(entry.clone()), Some(self.live_until)))),
            Some(None) => Ok(None),
            None => {
                self.missing.borrow_mut().push(key.as_ref().clone());
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
