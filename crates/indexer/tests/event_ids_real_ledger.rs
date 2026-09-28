//! `parse_ledger` gives every consensus event of a real ledger its
//! stellar-rpc id (task 0541, ADR 0059).
//!
//! Fixture: ledger 58,816,920 (protocol 23, end-of-ledger refunds) — the
//! `LedgerCloseMetaBatch` object from the public archive
//! `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/FC7E89FF--58816000-58879999/FC7E8667--58816920.xdr.zst`.

use std::collections::HashSet;

use xdr_parser::{EventId, EventSource};

#[test]
fn every_consensus_event_of_a_real_ledger_has_a_unique_rpc_id() {
    // SAFETY: single-threaded before any parse; parse_ledger's cold-start
    // contract needs the passphrase.
    unsafe {
        std::env::set_var(
            "STELLAR_NETWORK_PASSPHRASE",
            "Public Global Stellar Network ; September 2015",
        );
    }
    indexer::handler::process::init_network_id().expect("network id");
    let raw = include_bytes!("fixtures/ledger_58816920.xdr.zst");
    let xdr = xdr_parser::decompress_zstd(raw).expect("zstd");
    let batch = xdr_parser::deserialize_batch(&xdr).expect("batch");
    let out = indexer::handler::process::parse_ledger(&batch.ledger_close_metas[0]);

    let ids: Vec<EventId> = out
        .events
        .iter()
        .flat_map(|(_, evs)| evs)
        .filter(|e| e.source != EventSource::Diagnostic)
        .map(|e| e.event_id.expect("consensus event without an rpc id"))
        .collect();
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "two events share an rpc id");

    // Charges and end-of-ledger refunds are numbered 0..n per ledger, no gaps.
    let counters = |tx: u32| {
        let mut n: Vec<u32> = ids
            .iter()
            .filter(|id| id.transaction_index == tx)
            .map(|id| id.event_index)
            .collect();
        n.sort_unstable();
        n
    };
    let charges = counters(EventId::BEFORE_ALL_TXS);
    let refunds = counters(EventId::AFTER_ALL_TXS);
    assert_eq!(charges, (0..charges.len() as u32).collect::<Vec<_>>());
    assert_eq!(refunds, (0..refunds.len() as u32).collect::<Vec<_>>());
    assert_eq!(charges.len(), out.transactions.len());
    assert!(!refunds.is_empty(), "protocol 23 ledger with refunds");
    assert!(
        ids.iter()
            .all(|id| id.operation_index != EventId::AFTER_TX_OPERATION),
        "protocol 23 refunds are AfterAllTxs, never AfterTx"
    );
}
