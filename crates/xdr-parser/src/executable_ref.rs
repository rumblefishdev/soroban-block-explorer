//! CAP-85 external contract executables (protocol 28).
//!
//! From protocol 28 a contract's code can be a *reference* rather than a hash:
//! the instance names `(owner contract, tag)`, and the 32-byte Wasm hash lives
//! in the owner's own persistent storage under that tag. One entry serves a
//! whole fleet, so re-pointing it upgrades every member atomically — which is
//! the entire purpose of the CAP.
//!
//! Two facts fall out of that, and this module keeps them apart on purpose:
//!
//! 1. **What the instance says** — `(owner, tag)`. Changes only when the
//!    contract itself is created or updated.
//! 2. **What the tag currently resolves to** — a Wasm hash in the owner's
//!    storage. Changes when the OWNER writes, with no transaction, event or
//!    ledger change touching the member contract at all.
//!
//! Resolving (2) into the member's row at write time would mean one owner
//! write forcing a rewrite of every member — the fan-out that makes a stored
//! hash go stale invisibly, which is the defect class of tasks 0320/0326. So
//! the reference is stored as stated and the mapping is stored separately;
//! "which code does this contract run" is answered by joining them at read.
//!
//! The protocol guarantees make the mapping well-behaved: the entry cannot be
//! deleted (`del_contract_data` panics for a tag key), and any value written
//! must be the hash of an already-uploaded `ContractCode`.

use serde_json::Value;

use crate::types::ExtractedLedgerEntryChange;

/// The reference a contract instance carries in place of its own Wasm hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableRef {
    /// `C…` StrKey of the contract whose storage holds the hash.
    ///
    /// The XDR field is a bare `SCAddress`, so an ACCOUNT address is
    /// syntactically expressible — but the host reads the hash out of "the
    /// `executable_owner` contract storage entry keyed by `tag`" and panics if
    /// no such entry exists. Accounts have no contract-data storage, so such a
    /// reference can never be created and can never reach a ledger. Kept as the
    /// StrKey the instance actually stated rather than a contract-typed value,
    /// so a future protocol widening this does not corrupt what we store.
    pub owner: String,
    /// The tag naming which of the owner's executables this contract runs.
    pub tag: String,
}

/// One `(owner, tag) → wasm_hash` mapping as of a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedExecutableRefTarget {
    pub owner: String,
    pub tag: String,
    /// Hex-encoded 32-byte Wasm hash, matching `wasm_hash` elsewhere.
    pub wasm_hash: String,
    pub ledger_sequence: u32,
}

/// The `(owner, tag)` a contract instance points at, or `None` when the
/// instance carries its own executable (Wasm or Stellar-asset).
///
/// Reads the tagged JSON produced by `scval::scval_to_typed_json` for the
/// instance's `executable`, which renders an external ref as
/// `{"type":"external_ref","owner":…,"tag":…}`.
pub fn external_ref_from_instance(data: &Value) -> Option<ExecutableRef> {
    let executable = data.get("val")?.get("value")?.get("executable")?;
    if executable.get("type")?.as_str()? != "external_ref" {
        return None;
    }
    Some(ExecutableRef {
        owner: executable.get("owner")?.as_str()?.to_string(),
        tag: executable.get("tag")?.as_str()?.to_string(),
    })
}

/// Every `(owner, tag) → wasm_hash` mapping written in this batch.
///
/// An executable-reference entry is a persistent `contract_data` entry whose
/// KEY is an `SCV_EXECUTABLE_TAG` value (protocol 28's new `ScVal` arm) and
/// whose value is 32 bytes. The protocol admits no other shape under such a
/// key, so anything else is ignored rather than guessed at.
///
/// `removed` changes are not considered: the protocol forbids deleting these
/// entries, so a mapping only ever appears or is re-pointed.
///
/// **One row per `(owner, tag, ledger)`, the LAST one seen.** Nothing stops an
/// owner re-pointing the same tag twice in a single ledger — two transactions,
/// or one transaction calling twice — and the storage row is versioned by
/// ledger. ClickHouse keeps the most recently INSERTED row on a version tie,
/// which is physical insert order, not chain order, and only at a merge that
/// may never come. Folding here with the shared `fold::keep_last_by_key` — the
/// same helper the pool-state tables use for the same reason — collapses the
/// writes inside the `changes` it is given. The caller passes one transaction
/// at a time, so the writer (`build_executable_ref_rows`) folds again across
/// the whole ledger.
pub fn extract_executable_ref_targets(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedExecutableRefTarget> {
    let mut out: Vec<ExtractedExecutableRefTarget> = Vec::new();

    for change in changes {
        if change.entry_type != "contract_data" {
            continue;
        }
        // `state` is the pre-image half of an update pair — taking it too would
        // write the OLD hash after the new one within the same ledger.
        if !matches!(
            change.change_type.as_str(),
            "created" | "updated" | "restored"
        ) {
            continue;
        }
        let Some(data) = change.data.as_ref() else {
            continue;
        };
        let Some(tag) = executable_tag_key(data) else {
            continue;
        };
        let Some(owner) = data.get("contract").and_then(Value::as_str) else {
            continue;
        };
        let Some(wasm_hash) = wasm_hash_value(data) else {
            continue;
        };

        out.push(ExtractedExecutableRefTarget {
            owner: owner.to_string(),
            tag,
            wasm_hash,
            ledger_sequence: change.ledger_sequence,
        });
    }

    crate::fold::keep_last_by_key(out, |t| (t.owner.clone(), t.tag.clone(), t.ledger_sequence))
}

/// The tag string when this entry's key is an executable tag; `None` otherwise.
fn executable_tag_key(data: &Value) -> Option<String> {
    let key = data.get("key")?;
    if key.get("type")?.as_str()? != "executable_tag" {
        return None;
    }
    Some(key.get("value")?.as_str()?.to_string())
}

/// The stored value as a hex Wasm hash. The value is base64 `bytes` in the
/// tagged JSON; anything that is not exactly 32 bytes is not a Wasm hash and
/// is dropped rather than padded or truncated into one.
fn wasm_hash_value(data: &Value) -> Option<String> {
    use base64::Engine;

    let val = data.get("val")?;
    if val.get("type")?.as_str()? != "bytes" {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(val.get("value")?.as_str()?)
        .ok()?;
    (bytes.len() == 32).then(|| hex::encode(bytes))
}

#[cfg(test)]
#[path = "executable_ref_tests.rs"]
mod tests;
