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
use xdr_parser::{EventId, EventOrigin, LedgerEvents};

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
fn the_fee_events_carry_their_stage_through_the_parser() {
    let meta = meta();
    let events = LedgerEvents::new(62_032_880, 0, &[&meta])
        .extract(0, "0a120260")
        .events;

    // This is the path the API serves.
    let stages: Vec<_> = events
        .iter()
        .filter_map(|e| match e.origin {
            EventOrigin::Transaction(stage) => Some(stage),
            EventOrigin::Operation(_) => None,
        })
        .collect();
    assert_eq!(
        stages,
        vec![
            TransactionEventStage::BeforeAllTxs,
            TransactionEventStage::AfterAllTxs,
        ]
    );
}

#[test]
fn the_rpc_id_orders_charge_operation_refund() {
    let meta = meta();
    let events = LedgerEvents::new(62_032_880, 0, &[&meta])
        .extract(0, "0a120260")
        .events;

    let id = |origin: &dyn Fn(EventOrigin) -> bool| {
        events
            .iter()
            .find(|e| origin(e.origin))
            .map(|e| e.event_id)
            .expect("the fixture carries the event")
    };
    let charge = id(&|o| o == EventOrigin::Transaction(TransactionEventStage::BeforeAllTxs));
    let op = id(&|o| matches!(o, EventOrigin::Operation(_)));
    let refund = id(&|o| o == EventOrigin::Transaction(TransactionEventStage::AfterAllTxs));

    // Execution order: the refund is settled after every transaction, though
    // the meta lists it ahead of the operation it refunds.
    assert!(charge < op && op < refund);
    assert_eq!(
        (refund.transaction_index, refund.operation_index),
        (EventId::AFTER_ALL_TXS, 0)
    );
}
