//! Thin wrappers over `xdr_parser::extract_*` that pick out the heavy-field
//! subset each endpoint needs.
//!
//! The wrappers are **pure functions** on already-fetched `LedgerCloseMeta`
//! — no I/O, no DB, no state. They slice the parsed ledger down to the
//! heavy-payload shape defined in `dto.rs`.
//!
//! Endpoints (wired in a follow-up task) call these after `StellarArchiveFetcher` hands
//! back the `LedgerCloseMeta` from the public archive.

use stellar_xdr::{LedgerCloseMeta, TransactionEnvelope, TransactionMeta};
use tracing::instrument;

use super::dto::{E3HeavyFields, SignatureDto, XdrEventDto, XdrOperationDto};

/// Extract the heavy-field subset of the E3 (`/transactions/:hash`) response
/// for a given transaction hash within the supplied ledger.
///
/// `tx_hash` is the lowercase hex (64 chars) transaction hash — same format
/// as `ExtractedTransaction.hash` produced by `xdr_parser::extract_transactions`.
///
/// Returns `None` when the ledger does not contain a transaction matching
/// `tx_hash`. The calling handler should treat that as "no heavy fields
/// available" and fall back to DB-only response with
/// `heavy_fields_status: unavailable`.
#[instrument(skip(meta, network_id), fields(tx_hash = %tx_hash))]
pub fn extract_e3_heavy(
    meta: &LedgerCloseMeta,
    tx_hash: &str,
    network_id: &[u8; 32],
) -> Option<E3HeavyFields> {
    let ledger = xdr_parser::extract_ledger(meta);
    let ledger_seq = ledger.sequence;
    let closed_at = ledger.closed_at;

    let extracted_txs = xdr_parser::extract_transactions(meta, ledger_seq, closed_at, network_id);
    let envelopes = xdr_parser::envelope::extract_envelopes(meta, network_id);
    let tx_metas = collect_tx_metas(meta);

    let (idx, ext_tx) = extracted_txs
        .iter()
        .enumerate()
        .find(|(_, t)| t.hash == tx_hash)?;

    let envelope = envelopes.get(idx).and_then(Option::as_ref);
    let tx_meta = tx_metas.get(idx).copied();

    // Envelope-level details: signatures + fee-bump source.
    let (signatures, fee_bump_source) = envelope
        .map(|env| (envelope_signatures(env), envelope_fee_bump_source(env)))
        .unwrap_or_default();

    // Events: contract + diagnostic together, none without tx meta. Fee events
    // are numbered per ledger, so the ids need every meta of it.
    let (contract_events, diagnostic_events) = event_dtos(
        xdr_parser::LedgerEvents::new(ledger_seq, closed_at, &tx_metas).extract(idx, &ext_tx.hash),
    );

    // Invocations: nested Soroban call tree (flat list is not exposed by any endpoint).
    let operation_tree = match (envelope, tx_meta) {
        (Some(env), Some(tm)) => {
            let inner = xdr_parser::envelope::inner_transaction(env);
            xdr_parser::extract_invocations(
                &inner,
                Some(tm),
                &ext_tx.hash,
                ledger_seq,
                closed_at,
                &ext_tx.source_account,
                ext_tx.successful,
            )
            .operation_tree
        }
        _ => None,
    };

    // Operations: raw details per op. Op results feed the path-payment
    // pool claims (poolIds / claimedAtoms in details — task 0261), so that
    // path stays success-gated; per-op result CODES come from all applied
    // arms — on a failed tx the failing op's code is the fail reason
    // (task 0352).
    let tx_results = xdr_parser::collect_tx_results(meta);
    let op_results = tx_results
        .get(idx)
        .and_then(|r| xdr_parser::tx_op_results(r));
    let any_op_results = tx_results
        .get(idx)
        .and_then(|r| xdr_parser::tx_op_results_any(r));
    let operations = envelope
        .map(|env| {
            let inner = xdr_parser::envelope::inner_transaction(env);
            xdr_parser::extract_operations(
                &inner,
                tx_meta,
                op_results,
                &ext_tx.hash,
                ledger_seq,
                idx,
            )
            .into_iter()
            .filter_map(|op| {
                // operation_index is 1-based (Horizon convention); the XDR
                // result array is 0-based.
                let result_code = any_op_results
                    .and_then(|rs| rs.get((op.operation_index as usize).saturating_sub(1)))
                    .map(|r| xdr_parser::op_result_code(r).to_string());
                to_operation_dto(op, result_code)
            })
            .collect()
        })
        .unwrap_or_default();

    Some(E3HeavyFields {
        memo_type: ext_tx.memo_type.clone(),
        memo: ext_tx.memo.clone(),
        signatures,
        fee_bump_source,
        envelope_xdr: Some(ext_tx.envelope_xdr.clone()).filter(|s| !s.is_empty()),
        result_xdr: Some(ext_tx.result_xdr.clone()).filter(|s| !s.is_empty()),
        result_meta_xdr: ext_tx.result_meta_xdr.clone().filter(|s| !s.is_empty()),
        diagnostic_events,
        contract_events,
        operations,
        result_code: if ext_tx.parse_error {
            None
        } else {
            Some(ext_tx.result_code.clone())
        },
        operation_tree,
    })
}

// --- private helpers ---

/// Checked `u32 → i16` conversion for an operation's `application_order`,
/// which correlates to a DB `SMALLINT` column.
/// Returns `None` and logs a warning if the value overflows i16 — the caller
/// skips the row rather than silently truncate and corrupt correlation with DB.
fn to_i16_index(value: u32, kind: &'static str) -> Option<i16> {
    match i16::try_from(value) {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::warn!(
                kind,
                value,
                "index out of SMALLINT range — skipping row to avoid silent truncation"
            );
            None
        }
    }
}

/// Collect borrowed `&TransactionMeta` references for every transaction in
/// the ledger, in the same order as `tx_processing` (i.e. index `i` in the
/// returned `Vec` corresponds to the `i`-th entry of `tx_processing`, which
/// also matches `xdr_parser::extract_transactions` and
/// `xdr_parser::envelope::extract_envelopes` output ordering — callers rely
/// on this alignment when joining metas back to extracted txs by index).
/// Mirrors the unified collection used in
/// `crates/indexer/src/handler/process.rs::collect_tx_metas`.
fn collect_tx_metas(meta: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
    }
}

fn envelope_signatures(env: &TransactionEnvelope) -> Vec<SignatureDto> {
    let sigs: &[stellar_xdr::DecoratedSignature] = match env {
        TransactionEnvelope::TxV0(v0) => &v0.signatures,
        TransactionEnvelope::Tx(v1) => &v1.signatures,
        TransactionEnvelope::TxFeeBump(fb) => &fb.signatures,
    };
    sigs.iter()
        .map(|s| SignatureDto {
            hint: hex::encode(s.hint.0),
            signature: hex::encode(&s.signature.0),
        })
        .collect()
}

fn envelope_fee_bump_source(env: &TransactionEnvelope) -> Option<String> {
    match env {
        TransactionEnvelope::TxFeeBump(fb) => Some(fb.tx.fee_source.to_string()),
        _ => None,
    }
}

/// Wire spelling of CAP-67's `TransactionEventStage`. Snake_case to match the
/// rest of the DTO surface; the variants are the XDR's, not ours.
fn stage_name(stage: stellar_xdr::TransactionEventStage) -> String {
    use stellar_xdr::TransactionEventStage as S;
    match stage {
        S::BeforeAllTxs => "before_all_txs",
        S::AfterTx => "after_tx",
        S::AfterAllTxs => "after_all_txs",
    }
    .to_string()
}

fn event_dtos(tx: xdr_parser::TxEvents) -> (Vec<XdrEventDto>, Vec<XdrEventDto>) {
    use xdr_parser::EventOrigin;

    let mut contract: Vec<XdrEventDto> = tx
        .events
        .into_iter()
        .map(|e| {
            let (operation_index, stage) = match e.origin {
                EventOrigin::Operation(op) => (i16::try_from(op).ok(), None),
                EventOrigin::Transaction(stage) => (None, Some(stage_name(stage))),
            };
            XdrEventDto {
                event_type: e.event_type.to_string(),
                contract_id: e.contract_id,
                topics: topics_to_vec(e.topics),
                data: e.data,
                id: Some(e.event_id.to_rpc_string()),
                // From the origin, not the id: a fee event's id names operation
                // 0 or 4095, and the operation cards would take it as their own.
                operation_index,
                event_index: Some(e.event_id.event_index),
                stage,
            }
        })
        .collect();
    // Execution order. The id strings are fixed-width, so string order is the
    // numeric order.
    contract.sort_by(|a, b| a.id.cmp(&b.id));
    // The debug channel keeps its container order and has no id.
    let diagnostic = tx
        .diagnostic
        .into_iter()
        .map(|e| XdrEventDto {
            event_type: e.event_type.to_string(),
            contract_id: e.contract_id,
            topics: topics_to_vec(e.topics),
            data: e.data,
            id: None,
            operation_index: None,
            event_index: None,
            stage: None,
        })
        .collect();
    (contract, diagnostic)
}

fn topics_to_vec(topics: serde_json::Value) -> Vec<serde_json::Value> {
    match topics {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    }
}

fn to_operation_dto(
    op: xdr_parser::ExtractedOperation,
    result_code: Option<String>,
) -> Option<XdrOperationDto> {
    let application_order = to_i16_index(op.operation_index, "application_order")?;
    Some(XdrOperationDto {
        op_type: op.op_type.to_string(),
        application_order,
        details: op.details,
        result_code,
    })
}

#[cfg(test)]
mod tests;
