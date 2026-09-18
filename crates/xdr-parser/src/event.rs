//! CAP-67 event extraction from Soroban transaction metadata.
//!
//! Extracts contract, system, and diagnostic events from `SorobanTransactionMeta`.
//! Each event is decoded into an `ExtractedEvent` with ScVal-decoded topics and data.

use base64::Engine;
use serde_json::{Value, json};
use stellar_xdr::*;

use crate::scval::scval_to_typed_json;
use crate::types::{EventSource, ExtractedEvent};
use domain::ContractEventType as DomainEventType;

/// Extract all events from a transaction's metadata.
///
/// Returns one `ExtractedEvent` per event in `SorobanTransactionMeta.events`.
/// Returns an empty vec for non-Soroban transactions (no V3/V4 meta).
pub fn extract_events(
    tx_meta: &TransactionMeta,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
) -> Vec<ExtractedEvent> {
    match tx_meta {
        TransactionMeta::V3(v3) => {
            let Some(ref meta) = v3.soroban_meta else {
                return Vec::new();
            };
            let mut extracted: Vec<ExtractedEvent> = meta
                .events
                .iter()
                .enumerate()
                .map(|(i, event)| {
                    extract_single_event(
                        event,
                        transaction_hash,
                        ledger_sequence,
                        created_at,
                        i,
                        EventSource::TxLevel,
                    )
                })
                .collect();
            // Include diagnostic_events (separate field in SorobanTransactionMeta)
            let base = extracted.len();
            for (j, diag) in meta.diagnostic_events.iter().enumerate() {
                extracted.push(extract_single_event(
                    &diag.event,
                    transaction_hash,
                    ledger_sequence,
                    created_at,
                    base + j,
                    EventSource::Diagnostic,
                ));
            }
            extracted
        }
        TransactionMeta::V4(v4) => {
            // CAP-67 (Protocol 23+) reorganises events into three locations
            // — tx-level (fee charge and refund, carrying a `stage`),
            // per-operation (Soroban contract events emitted during
            // InvokeHostFunction execution + classic-op SAC events under
            // Protocol 23 unification), and diagnostic. `position_in_tx` is
            // numbered sequentially across all three sources so the V3
            // contract (monotonic per-tx index) is preserved. Each event
            // is tagged with its `EventSource` so consumers can drop the
            // diagnostic container without trusting the inner `type_`
            // (the diagnostic container holds byte-identical Contract-typed
            // copies of per-op consensus events when diagnostic mode is
            // enabled — task 0182).
            let mut extracted: Vec<ExtractedEvent> = v4
                .events
                .iter()
                .enumerate()
                .map(|(i, tx_event)| {
                    // CAP-67 gives tx-level events a `stage`, and it is the
                    // only statement of WHEN they happened. Measured on
                    // mainnet, not inferred (`tests/tx_event_stage_real_meta`):
                    // the fee charge is `BeforeAllTxs`, the refund
                    // `AfterAllTxs` — settled after every transaction in the
                    // ledger, yet numbered ahead of the operation it refunds.
                    // Without the stage, `position_in_tx` reads as a timeline
                    // it is not.
                    let mut ev = extract_single_event(
                        &tx_event.event,
                        transaction_hash,
                        ledger_sequence,
                        created_at,
                        i,
                        EventSource::TxLevel,
                    );
                    ev.stage = Some(tx_event.stage);
                    ev
                })
                .collect();

            let mut next_idx = extracted.len();
            for (op_i, op_meta) in v4.operations.iter().enumerate() {
                for (pos, event) in op_meta.events.iter().enumerate() {
                    let mut ev = extract_single_event(
                        event,
                        transaction_hash,
                        ledger_sequence,
                        created_at,
                        next_idx,
                        EventSource::PerOp,
                    );
                    // Keep the envelope position — it is the only place the
                    // meta states which operation emitted the event (D7) —
                    // and the position inside that operation, which with it
                    // forms the official event identity (task 0540).
                    ev.op_index = u32::try_from(op_i).ok();
                    ev.event_pos_in_op = u32::try_from(pos).ok();
                    extracted.push(ev);
                    next_idx += 1;
                }
            }

            for diag in v4.diagnostic_events.iter() {
                extracted.push(extract_single_event(
                    &diag.event,
                    transaction_hash,
                    ledger_sequence,
                    created_at,
                    next_idx,
                    EventSource::Diagnostic,
                ));
                next_idx += 1;
            }

            extracted
        }
        _ => Vec::new(),
    }
}

/// Extract a single `ContractEvent` into an `ExtractedEvent`.
fn extract_single_event(
    event: &ContractEvent,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    index: usize,
    source: EventSource,
) -> ExtractedEvent {
    // ADR 0031: emit the typed enum directly; persist binds it as SMALLINT.
    let event_type = match event.type_ {
        ContractEventType::System => DomainEventType::System,
        ContractEventType::Contract => DomainEventType::Contract,
        ContractEventType::Diagnostic => DomainEventType::Diagnostic,
    };

    let contract_id = event
        .contract_id
        .as_ref()
        .map(|id| ScAddress::Contract(id.clone()).to_string());

    let (topics, data) = match &event.body {
        ContractEventBody::V0(v0) => {
            let topics: Vec<Value> = v0.topics.iter().map(scval_to_typed_json).collect();
            let data = scval_to_typed_json(&v0.data);
            (json!(topics), data)
        }
    };

    ExtractedEvent {
        transaction_hash: transaction_hash.to_string(),
        event_type,
        source,
        contract_id,
        topics,
        data,
        position_in_tx: u32::try_from(index).expect("event index does not fit into u32"),
        op_index: None,
        event_pos_in_op: None,
        // Only `v4.events` carries one; the tx-level arm sets it.
        stage: None,
        // Needs the transaction's position in the ledger; `assign_event_ids`.
        event_id: None,
        ledger_sequence,
        created_at,
    }
}

/// stellar-rpc's identity for a non-diagnostic event (ADR 0059): a TOID
/// (SEP-35 layout) plus an event number. Source: stellar-rpc
/// `internal/db/event.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId {
    pub ledger_sequence: u32,
    pub transaction_index: u32,
    pub operation_index: u16,
    pub event_index: u32,
}

impl EventId {
    /// Transaction part of a `BeforeAllTxs` event.
    pub const BEFORE_ALL_TXS: u32 = 0;
    /// Transaction part of an `AfterAllTxs` event: the largest 20-bit value.
    pub const AFTER_ALL_TXS: u32 = (1 << 20) - 1;
    /// Operation part of an `AfterTx` event: the largest 12-bit value.
    pub const AFTER_TX_OPERATION: u16 = (1 << 12) - 1;

    pub fn toid(&self) -> u64 {
        (u64::from(self.ledger_sequence) << 32)
            | (u64::from(self.transaction_index) << 12)
            | u64::from(self.operation_index)
    }

    /// The `id` string `getEvents` returns.
    pub fn to_rpc_string(&self) -> String {
        format!("{:019}-{:010}", self.toid(), self.event_index)
    }
}

/// Ids of every transaction-level event of a ledger, per transaction, in
/// `TransactionMetaV4.events` order. `tx_metas[i]` is application order
/// `i + 1`. Counters come from the meta itself, so a transaction whose events
/// were not extracted still advances them. Non-V4 metas have none.
pub fn tx_level_event_ids(
    ledger_sequence: u32,
    tx_metas: &[&TransactionMeta],
) -> Vec<Vec<EventId>> {
    let mut before_all = 0u32;
    let mut after_all = 0u32;
    let mut out = Vec::with_capacity(tx_metas.len());
    for (i, meta) in tx_metas.iter().enumerate() {
        let application_order = u32::try_from(i + 1).expect("transactions per ledger fit u32");
        let mut ids = Vec::new();
        if let TransactionMeta::V4(v4) = meta {
            let mut after_tx = 0u32;
            for event in v4.events.iter() {
                let (transaction_index, operation_index, counter) = match event.stage {
                    TransactionEventStage::BeforeAllTxs => {
                        (EventId::BEFORE_ALL_TXS, 0, &mut before_all)
                    }
                    TransactionEventStage::AfterTx => (
                        application_order,
                        EventId::AFTER_TX_OPERATION,
                        &mut after_tx,
                    ),
                    TransactionEventStage::AfterAllTxs => {
                        (EventId::AFTER_ALL_TXS, 0, &mut after_all)
                    }
                };
                ids.push(EventId {
                    ledger_sequence,
                    transaction_index,
                    operation_index,
                    event_index: *counter,
                });
                *counter += 1;
            }
        }
        out.push(ids);
    }
    out
}

/// Sets `event_id` on one transaction's `extract_events` output. `tx_level` is
/// that transaction's entry from [`tx_level_event_ids`]. Diagnostic events,
/// per-operation events without a position and transaction-level events
/// without a matching id keep `None`; staging refuses those rows.
pub fn assign_event_ids(
    ledger_sequence: u32,
    application_order: u32,
    tx_level: &[EventId],
    events: &mut [ExtractedEvent],
) {
    let mut tx_level = tx_level.iter().copied();
    for event in events.iter_mut() {
        event.event_id = match event.source {
            EventSource::Diagnostic => None,
            EventSource::TxLevel if event.stage.is_some() => tx_level.next(),
            EventSource::TxLevel => None,
            EventSource::PerOp => match (event.op_index, event.event_pos_in_op) {
                (Some(op), Some(pos)) => u16::try_from(op).ok().map(|operation_index| EventId {
                    ledger_sequence,
                    transaction_index: application_order,
                    operation_index,
                    event_index: pos,
                }),
                _ => None,
            },
        };
    }
}

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
