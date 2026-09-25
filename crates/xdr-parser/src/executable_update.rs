//! The `executable_update` system event: what a contract's code became.

use base64::Engine;
use serde_json::Value;

/// What an `executable_update` system event set the contract's code to.
///
/// Per CAP-0046-05 a Wasm upgrade emits topics
/// `[Symbol("executable_update"), <old executable>, <new executable>]`, each
/// executable encoded as a contract-type `ContractExecutable` SCVal. Protocol
/// 28 (CAP-85) adds a third arm to that enum and reuses the SAME event for
/// `update_current_contract_executable_ref`, so an upgrade can now hand the
/// contract's code over to another contract entirely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutableUpdate {
    /// `vec[Symbol("Wasm"), Bytes(hash)]` — the contract now runs this code.
    Wasm([u8; 32]),
    /// `vec[Symbol("ExternalRef"), map{owner, tag}]` — the contract now runs
    /// whatever the owner's tag points at, and carries no hash of its own.
    /// The exact shape is given in CAP-85.
    ExternalRef { owner: String, tag: String },
}

/// Read an `executable_update` system-event `topics` array (the typed JSON
/// stored in `soroban_events.topics_xdr`).
///
/// `None` when the shape does not match: a different topic name, a
/// `StellarAsset` executable (a SAC never upgrades), or a malformed payload.
///
/// A contract may move freely between a direct hash and a reference, so BOTH
/// arms have to be handled by the caller: treating an `ExternalRef` upgrade as
/// "nothing happened" leaves the previously stored `wasm_hash` in place, which
/// is the stale-hash defect of tasks 0320/0326 wearing a new hat.
pub fn extract_executable_update(topics: &Value) -> Option<ExecutableUpdate> {
    let arr = topics.as_array()?;
    // topic[0] is the event-name symbol.
    if arr.first()?.get("value")?.as_str()? != "executable_update" {
        return None;
    }
    // topic[2] is the NEW executable.
    let new_exec = arr.get(2)?.get("value")?.as_array()?;
    match new_exec.first()?.get("value")?.as_str()? {
        "Wasm" => {
            let b64 = new_exec.get(1)?.get("value")?.as_str()?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            Some(ExecutableUpdate::Wasm(bytes.try_into().ok()?))
        }
        "ExternalRef" => {
            let fields = new_exec.get(1)?.get("value")?.as_array()?;
            let field = |name: &str| {
                fields.iter().find_map(|e| {
                    (e.get("key")?.get("value")?.as_str()? == name)
                        .then(|| e.get("value")?.get("value")?.as_str().map(str::to_string))?
                })
            };
            Some(ExecutableUpdate::ExternalRef {
                owner: field("owner")?,
                tag: field("tag")?,
            })
        }
        // A `StellarAsset` executable, or an arm a later protocol adds. Neither
        // is guessed at.
        _ => None,
    }
}

#[cfg(test)]
mod tests;
