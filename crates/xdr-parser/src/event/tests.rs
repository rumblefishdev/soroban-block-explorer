use stellar_xdr::{
    ContractEventV0, ContractId, DiagnosticEvent as XdrDiagnostic, ExtensionPoint, Hash,
    LedgerEntryChanges, OperationMetaV2, ScSymbol, ScVal, SorobanTransactionMeta,
    SorobanTransactionMetaExt, TransactionMetaV3, TransactionMetaV4, VecM,
};

use super::*;

fn contract_event(contract_byte: u8, data: u32) -> ContractEvent {
    ContractEvent {
        ext: ExtensionPoint::V0,
        contract_id: Some(ContractId(Hash([contract_byte; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: VecM::default(),
            data: ScVal::U32(data),
        }),
    }
}

fn tx_event(stage: TransactionEventStage, data: u32) -> TransactionEvent {
    TransactionEvent {
        stage,
        event: contract_event(0xAA, data),
    }
}

fn op(events: Vec<ContractEvent>) -> OperationMetaV2 {
    OperationMetaV2 {
        ext: ExtensionPoint::V0,
        changes: LedgerEntryChanges::default(),
        events: events.try_into().unwrap(),
    }
}

fn diag(event: ContractEvent) -> XdrDiagnostic {
    XdrDiagnostic {
        in_successful_contract_call: true,
        event,
    }
}

fn v4(
    tx_events: Vec<TransactionEvent>,
    operations: Vec<OperationMetaV2>,
    diagnostic: Vec<XdrDiagnostic>,
) -> TransactionMeta {
    TransactionMeta::V4(TransactionMetaV4 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: operations.try_into().unwrap(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
        events: tx_events.try_into().unwrap(),
        diagnostic_events: diagnostic.try_into().unwrap(),
    })
}

fn v3(events: Vec<ContractEvent>, diagnostic: Vec<XdrDiagnostic>) -> TransactionMeta {
    TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: Some(SorobanTransactionMeta {
            ext: SorobanTransactionMetaExt::V0,
            events: events.try_into().unwrap(),
            return_value: ScVal::Void,
            diagnostic_events: diagnostic.try_into().unwrap(),
        }),
    })
}

/// The events of a ledger holding this one transaction.
fn only(meta: &TransactionMeta) -> TxEvents {
    LedgerEvents::new(100, 1_700_000_000, &[meta]).extract(0, "abcd1234")
}

fn id(ledger_sequence: u32, tx: u32, op: u16, event: u32) -> EventId {
    EventId {
        ledger_sequence,
        transaction_index: tx,
        operation_index: op,
        event_index: event,
    }
}

fn data(events: &[ExtractedEvent]) -> Vec<u64> {
    events
        .iter()
        .map(|e| e.data["value"].as_u64().unwrap())
        .collect()
}

#[test]
fn decodes_type_contract_topics_and_data() {
    let event = ContractEvent {
        ext: ExtensionPoint::V0,
        contract_id: Some(ContractId(Hash([0xAA; 32]))),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: vec![
                ScVal::Symbol(ScSymbol::try_from("transfer".as_bytes().to_vec()).unwrap()),
                ScVal::Address(ScAddress::Contract(ContractId(Hash([0xBB; 32])))),
                ScVal::U64(100),
            ]
            .try_into()
            .unwrap(),
            data: ScVal::U64(42),
        }),
    };
    let tx = only(&v4(vec![], vec![op(vec![event])], vec![]));

    let e = &tx.events[0];
    assert_eq!(e.event_type, DomainEventType::Contract);
    assert_eq!(e.transaction_hash, "abcd1234");
    assert!(e.contract_id.as_ref().unwrap().starts_with('C'));
    assert_eq!(e.event_id, id(100, 1, 0, 0));
    assert_eq!(e.created_at, 1_700_000_000);
    let topics = e.topics.as_array().unwrap();
    assert_eq!(
        topics.iter().map(|t| t["type"].clone()).collect::<Vec<_>>(),
        ["sym", "address", "u64"]
    );
    assert_eq!(topics[0]["value"], "transfer");
    assert_eq!(e.data["type"], "u64");
    assert_eq!(e.data["value"], 42);
}

#[test]
fn a_system_event_may_have_no_contract() {
    let event = ContractEvent {
        ext: ExtensionPoint::V0,
        contract_id: None,
        type_: ContractEventType::System,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: VecM::default(),
            data: ScVal::Void,
        }),
    };
    let tx = only(&v4(vec![], vec![op(vec![event])], vec![]));
    assert_eq!(tx.events[0].event_type, DomainEventType::System);
    assert!(tx.events[0].contract_id.is_none());
    assert!(tx.events[0].topics.as_array().unwrap().is_empty());
}

/// stellar-rpc `InsertEvents`: the transaction-level events, then each
/// operation's; the debug channel apart.
#[test]
fn v4_comes_out_as_stellar_rpc_reads_it() {
    use TransactionEventStage::BeforeAllTxs;
    let meta = v4(
        vec![tx_event(BeforeAllTxs, 100)],
        vec![
            op(vec![contract_event(0xB0, 200), contract_event(0xB1, 201)]),
            op(vec![contract_event(0xC0, 300)]),
        ],
        vec![diag(contract_event(0xDD, 400))],
    );
    let tx = only(&meta);

    assert_eq!(data(&tx.events), [100, 200, 201, 300]);
    assert_eq!(
        tx.events.iter().map(|e| e.origin).collect::<Vec<_>>(),
        [
            EventOrigin::Transaction(BeforeAllTxs),
            EventOrigin::Operation(0),
            EventOrigin::Operation(0),
            EventOrigin::Operation(1),
        ]
    );
    assert_eq!(
        tx.events.iter().map(|e| e.event_id).collect::<Vec<_>>(),
        [
            id(100, 0, 0, 0),
            id(100, 1, 0, 0),
            id(100, 1, 0, 1),
            id(100, 1, 1, 0)
        ]
    );
    assert_eq!(tx.diagnostic.len(), 1);
    assert_eq!(tx.diagnostic[0].data["value"], 400);
}

/// Task 0182: core mirrors an operation's event into the diagnostic channel,
/// byte-identical, `type_` included. The mirror must never reach a consensus
/// reader.
#[test]
fn a_diagnostic_mirror_never_reaches_the_consensus_list() {
    let meta = v4(
        vec![],
        vec![op(vec![contract_event(0xAA, 42)])],
        vec![diag(contract_event(0xAA, 42))],
    );
    let tx = only(&meta);
    assert_eq!(tx.events.len(), 1);
    assert_eq!(tx.events[0].origin, EventOrigin::Operation(0));
    assert_eq!(tx.diagnostic.len(), 1);
    assert_eq!(tx.diagnostic[0].event_type, DomainEventType::Contract);
}

#[test]
fn an_operation_without_events_still_holds_its_place() {
    let meta = v4(
        vec![],
        vec![op(vec![]), op(vec![contract_event(0xB0, 7)])],
        vec![],
    );
    let tx = only(&meta);
    assert_eq!(tx.events.len(), 1);
    assert_eq!(tx.events[0].origin, EventOrigin::Operation(1));
    assert_eq!(tx.events[0].event_id, id(100, 1, 1, 0));
}

/// Charges and end-of-ledger refunds are numbered across the ledger; asking
/// for one transaction alone still gives it the ledger's numbers.
#[test]
fn fee_events_take_rpc_sentinels_and_ledger_counters() {
    use TransactionEventStage::{AfterAllTxs, BeforeAllTxs};
    let m1 = v4(
        vec![tx_event(BeforeAllTxs, 1), tx_event(AfterAllTxs, 2)],
        vec![op(vec![contract_event(0xB0, 3), contract_event(0xB0, 4)])],
        vec![],
    );
    let m2 = v4(vec![tx_event(BeforeAllTxs, 5)], vec![], vec![]);
    let m3 = v4(
        vec![tx_event(BeforeAllTxs, 6), tx_event(AfterAllTxs, 7)],
        vec![],
        vec![],
    );
    let metas = [&m1, &m2, &m3];
    let ledger = LedgerEvents::new(7, 0, &metas);
    let ids = |i| {
        ledger
            .extract(i, "")
            .events
            .iter()
            .map(|e| e.event_id)
            .collect::<Vec<_>>()
    };

    assert_eq!(ids(2), [id(7, 0, 0, 2), id(7, 1_048_575, 0, 1)]);
    assert_eq!(
        ids(0),
        [
            id(7, 0, 0, 0),
            id(7, 1_048_575, 0, 0),
            id(7, 1, 0, 0),
            id(7, 1, 0, 1)
        ]
    );
    assert_eq!(ids(1), [id(7, 0, 0, 1)]);
}

#[test]
fn a_refund_before_protocol_23_is_after_its_own_transaction() {
    use TransactionEventStage::{AfterTx, BeforeAllTxs};
    let meta = || {
        v4(
            vec![tx_event(BeforeAllTxs, 1), tx_event(AfterTx, 2)],
            vec![],
            vec![],
        )
    };
    let (m1, m2) = (meta(), meta());
    let metas = [&m1, &m2];
    let refund = LedgerEvents::new(9, 0, &metas).extract(1, "").events[1].clone();
    assert_eq!(refund.origin, EventOrigin::Transaction(AfterTx));
    assert_eq!(refund.event_id, id(9, 2, 4_095, 0));
}

/// stellar-go `GetTransactionEvents`: a V3 Soroban transaction is one
/// operation and has no transaction-level events.
#[test]
fn v3_is_one_operation() {
    let meta = v3(
        vec![contract_event(0xAA, 1), contract_event(0xAA, 2)],
        vec![diag(contract_event(0xDD, 3))],
    );
    let tx = only(&meta);
    assert_eq!(
        tx.events
            .iter()
            .map(|e| (e.origin, e.event_id))
            .collect::<Vec<_>>(),
        [
            (EventOrigin::Operation(0), id(100, 1, 0, 0)),
            (EventOrigin::Operation(0), id(100, 1, 0, 1)),
        ]
    );
    assert_eq!(tx.diagnostic.len(), 1);
}

#[test]
fn a_transaction_without_soroban_meta_has_no_events() {
    let meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });
    let tx = only(&meta);
    assert!(tx.events.is_empty() && tx.diagnostic.is_empty());
}

#[test]
fn a_transaction_the_ledger_does_not_have_has_no_events() {
    let meta = v4(vec![], vec![op(vec![contract_event(0xAA, 1)])], vec![]);
    let tx = LedgerEvents::new(1, 0, &[&meta]).extract(1, "");
    assert!(tx.events.is_empty() && tx.diagnostic.is_empty());
}

#[test]
fn rpc_string_matches_getevents_format() {
    // Ledger 64,450,000's first charge as mainnet getEvents returns it.
    assert_eq!(
        id(64_450_000, 0, 0, 0).to_rpc_string(),
        "0276810642227200000-0000000000"
    );
}
