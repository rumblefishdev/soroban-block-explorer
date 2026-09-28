//! `TransactionEvent.stage` against a real mainnet V4 meta.
//!
//! Written after a comment in `event.rs` asserted that a fee refund carries
//! `AfterTx`. It does not — the chain says `AfterAllTxs`. The claim had been
//! reasoned from CAP-67's prose and never observed, because the deployed API
//! did not return the field yet, so the page showed a dash and nothing
//! contradicted it. This test makes the protocol answer instead of us.
//!
//! Fixture: `result_meta_xdr` of mainnet transaction
//! `0a120260bb0fbe48903291b8606b3058fbfb95defd45e02e12cf0361ec6dc38e`
//! (ledger 62,032,880) — a one-operation KALE transfer, the transaction from
//! issue #378.

use base64::Engine;
use stellar_xdr::{Limits, ReadXdr, TransactionEventStage, TransactionMeta};
use xdr_parser::{EventId, EventSource, LedgerEvents, extract_events};

const META_B64: &str = include_str!("fixtures/tx_0a120260_meta_v4.b64");

fn meta() -> TransactionMeta {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(META_B64.trim())
        .expect("fixture is valid base64");
    TransactionMeta::from_xdr(&bytes, Limits::none()).expect("fixture is a valid TransactionMeta")
}

#[test]
fn fee_charge_is_before_all_txs_and_the_refund_is_after_all_txs() {
    let TransactionMeta::V4(v4) = meta() else {
        panic!("fixture must be a V4 meta — the stage field exists nowhere else");
    };

    let stages: Vec<TransactionEventStage> = v4.events.iter().map(|e| e.stage).collect();
    assert_eq!(
        stages,
        vec![
            TransactionEventStage::BeforeAllTxs,
            TransactionEventStage::AfterAllTxs,
        ],
        "the charge is taken before any transaction runs and the refund is \
         settled after ALL of them — not `AfterTx`, which is what the code \
         comment used to claim"
    );
}

#[test]
fn only_tx_level_events_carry_a_stage() {
    let events = extract_events(&meta(), "0a120260", 62_032_880, 0);

    // Both fee events are tx-level and both carry their stage through the
    // parser — this is the path the API serves.
    let tx_level: Vec<_> = events
        .iter()
        .filter(|e| e.source == EventSource::TxLevel)
        .map(|e| e.stage)
        .collect();
    assert_eq!(
        tx_level,
        vec![
            Some(TransactionEventStage::BeforeAllTxs),
            Some(TransactionEventStage::AfterAllTxs),
        ]
    );

    // Everything else has none, and we invent none. The per-operation
    // container states position via `op_index`; the diagnostic container
    // states nothing about time at all.
    assert!(
        events
            .iter()
            .filter(|e| e.source != EventSource::TxLevel)
            .all(|e| e.stage.is_none()),
        "per-op and diagnostic events carry no stage in the protocol"
    );
}

#[test]
fn the_refund_is_numbered_before_the_operation_it_refunds() {
    let events = extract_events(&meta(), "0a120260", 62_032_880, 0);

    let refund = events
        .iter()
        .find(|e| e.stage == Some(TransactionEventStage::AfterAllTxs))
        .expect("the fixture carries a refund");
    let first_op_event = events
        .iter()
        .find(|e| e.source == EventSource::PerOp)
        .expect("the fixture carries a per-operation event");

    // This is the whole reason the field is carried: `position_in_tx` follows
    // the XDR containers, so the refund — settled last — is numbered ahead of
    // the operation that caused it. The number is a position in the record,
    // the stage is the time.
    assert!(
        refund.position_in_tx < first_op_event.position_in_tx,
        "refund #{} should be numbered before the op event #{}",
        refund.position_in_tx,
        first_op_event.position_in_tx
    );
}

#[test]
fn the_rpc_id_orders_charge_operation_refund() {
    let meta = meta();
    let events = LedgerEvents::new(62_032_880, 0, &[&meta]).extract(0, "0a120260");

    let id = |pick: &dyn Fn(&xdr_parser::ExtractedEvent) -> bool| {
        events
            .iter()
            .find(|e| pick(e))
            .and_then(|e| e.event_id)
            .expect("event with an id")
    };
    let charge = id(&|e| e.stage == Some(TransactionEventStage::BeforeAllTxs));
    let op = id(&|e| e.source == EventSource::PerOp);
    let refund = id(&|e| e.stage == Some(TransactionEventStage::AfterAllTxs));

    // Execution order, which the flat position above gets wrong.
    assert!(charge < op && op < refund);
    assert_eq!(
        (refund.transaction_index, refund.operation_index),
        (EventId::AFTER_ALL_TXS, 0)
    );
    assert!(
        events
            .iter()
            .filter(|e| e.source == EventSource::Diagnostic)
            .all(|e| e.event_id.is_none())
    );
}
