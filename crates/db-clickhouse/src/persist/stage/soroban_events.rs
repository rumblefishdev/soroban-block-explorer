//! `soroban_events` rows staged per ledger: unfolded (ADR 0044 §4a), keyed by
//! the stellar-rpc id (ADR 0059), with the event name lifted into `signature`.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;
use xdr_parser::scval;
use xdr_parser::types::{EventOrigin, ExtractedEvent, ExtractedLedger};

use super::{StagedLedger, staging_err};
use crate::SchemaError;
use crate::persist::ids;
use crate::persist::rows::SorobanEventRow;

/// Returns the (contract, transaction position) of every operation event, for
/// `contract_activity`.
pub(super) fn rows(
    out: &mut StagedLedger,
    events: &[(String, Vec<ExtractedEvent>)],
    app_order_by_hash: &HashMap<String, i16>,
    ledger: &ExtractedLedger,
    ledger_sequence_i64: i64,
) -> Result<BTreeSet<(i64, i16)>, SchemaError> {
    let mut contract_orphan_dropped: usize = 0;
    // (contract, transaction) of every operation event, for
    // `contract_activity` below: the parser says where an event came from,
    // so a fee event is left out by its origin, not inferred from its id.
    let mut contract_txs: BTreeSet<(i64, i16)> = BTreeSet::new();
    for (tx_hash, evs) in events {
        let Some(&application_order) = app_order_by_hash.get(tx_hash) else {
            continue;
        };
        for ev in evs {
            let Some(contract_strkey) = &ev.body.contract_id else {
                contract_orphan_dropped += 1;
                continue;
            };
            let id = ev.event_id;
            let topics_xdr = serde_json::to_string(&ev.body.topics)
                .map_err(|e| staging_err(&format!("event topics serialize: {e}")))?;
            let data_xdr = serde_json::to_string(&ev.body.data)
                .map_err(|e| staging_err(&format!("event data serialize: {e}")))?;
            let signature = extract_event_signature(&ev.body.topics);
            let contract_id = ids::contract_id(contract_strkey);
            if matches!(ev.origin, EventOrigin::Operation(_)) {
                contract_txs.insert((contract_id, application_order));
            }
            out.event_rows.push(SorobanEventRow {
                contract_id,
                ledger_sequence: ledger_sequence_i64,
                transaction_index: id.transaction_index,
                operation_index: id.operation_index,
                event_index: id.event_index,
                application_order,
                event_type: ev.body.event_type as i16,
                signature,
                topics_xdr,
                data_xdr,
            });
        }
    }
    if contract_orphan_dropped > 0 {
        tracing::debug!(
            ledger_sequence = ledger.sequence,
            contract_orphan_dropped,
            staged = out.event_rows.len(),
            "CH soroban_events filtered"
        );
    }
    Ok(contract_txs)
}

/// Event NAME, lifted from the topics into the `signature` column (the cheap
/// `WHERE signature = 'transfer'` filter).
///
/// Three publishing conventions exist on mainnet (task 0517; measured
/// 2026-09-02 on two 1M-ledger windows of the then-NULL population, shapes
/// identical in both):
///
/// 1. `[Symbol(name), …]` — the dominant convention (SEP-41, the router
///    family, …). Unchanged.
/// 2. `[String(label), Symbol(name), …]` — a protocol label first, the name
///    second (SoroswapPair/Router/Aggregator, DeFindexVault, BlendStrategy).
///    The label is NOT copied anywhere: it sits verbatim in `topics_xdr`
///    forever — extract on demand, never copy.
/// 3. `[String(name), …]` where `topics[1]` is NOT a Symbol — the
///    Phoenix-family plain-&str convention (`("swap","sender")` publishes
///    two Strings): the FIRST topic is the name, the second discriminates
///    the field and stays in the topics for the protocol's decoder.
///
/// Known compromise in arm 3: a future protocol publishing
/// `[String(label), String(name)]` would get its label lifted as the name —
/// wrong but visible and verifiable per protocol, unlike the silent NULL it
/// replaces.
///
/// Anything else with a non-empty topic vector resolves nowhere: it keeps
/// NULL **and warns**, so the next convention surfaces as a count, never as
/// absence (the 0517 monitor; 100% of the measured NULL population had a
/// String first topic, so this arm is quiet today). An EMPTY topic vector
/// stays a silent NULL — there is no name to resolve.
pub(super) fn extract_event_signature(topics: &Value) -> Option<String> {
    let arr = topics.as_array()?;
    let first = arr.first()?;
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
    if let Some(name) = scval::typed_str(first, "sym").and_then(nonempty) {
        return Some(name);
    }
    if let Some(label_or_name) = scval::typed_str(first, "string").and_then(nonempty) {
        if let Some(name) = arr
            .get(1)
            .and_then(|t| scval::typed_str(t, "sym"))
            .and_then(nonempty)
        {
            return Some(name);
        }
        return Some(label_or_name);
    }
    tracing::warn!(
        topics = %topics,
        "event name unresolved — unknown topic convention (task 0517 monitor)"
    );
    None
}
