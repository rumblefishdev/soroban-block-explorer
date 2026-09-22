use super::*;

/// Ledger 58,816,920 (protocol 23, end-of-ledger refunds) from the public
/// archive — the indexer's fixture, shared rather than copied.
fn ledger() -> LedgerCloseMeta {
    let raw = include_bytes!("../../../../../indexer/tests/fixtures/ledger_58816920.xdr.zst");
    let xdr = xdr_parser::decompress_zstd(raw).expect("zstd");
    let batch = xdr_parser::deserialize_batch(&xdr).expect("batch");
    batch.ledger_close_metas[0].clone()
}

/// `(transaction part, operation part)` of an rpc id string.
fn toid_parts(id: &str) -> (u64, u64) {
    let toid: u64 = id[..19].parse().expect("toid");
    ((toid >> 12) & 0xF_FFFF, toid & 0xFFF)
}

#[test]
fn transaction_page_events_carry_rpc_ids_in_execution_order() {
    let meta = ledger();
    let net_id = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);
    let ledger_info = xdr_parser::extract_ledger(&meta);
    let txs = xdr_parser::extract_transactions(
        &meta,
        ledger_info.sequence,
        ledger_info.closed_at,
        &net_id,
    );

    // The first transaction with a charge, an operation event and a refund.
    let (application_order, heavy) = txs
        .iter()
        .enumerate()
        .filter_map(|(i, tx)| Some((i + 1, extract_e3_heavy(&meta, &tx.hash, &net_id)?)))
        .find(|(_, h)| {
            let has = |stage: &str| {
                h.contract_events
                    .iter()
                    .any(|e| e.stage.as_deref() == Some(stage))
            };
            has("before_all_txs")
                && has("after_all_txs")
                && h.contract_events
                    .iter()
                    .any(|e| e.operation_index.is_some())
        })
        .expect("a transaction with a charge, an operation event and a refund");

    let events = &heavy.contract_events;
    let first = events.first().unwrap();
    let last = events.last().unwrap();
    assert_eq!(first.stage.as_deref(), Some("before_all_txs"));
    assert_eq!(toid_parts(first.id.as_deref().unwrap()), (0, 0));
    assert_eq!(last.stage.as_deref(), Some("after_all_txs"));
    assert_eq!(toid_parts(last.id.as_deref().unwrap()), (1_048_575, 0));

    for e in events.iter().filter(|e| e.operation_index.is_some()) {
        let (tx, op) = toid_parts(e.id.as_deref().unwrap());
        assert_eq!(tx, application_order as u64);
        assert_eq!(op, e.operation_index.unwrap() as u64);
        assert_eq!(
            e.event_index,
            Some(e.id.as_deref().unwrap()[20..].parse().unwrap())
        );
    }
    let ids: Vec<_> = events.iter().map(|e| e.id.clone()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "consensus events in rpc id order");
    assert!(heavy.diagnostic_events.iter().all(|e| e.id.is_none()));
}
