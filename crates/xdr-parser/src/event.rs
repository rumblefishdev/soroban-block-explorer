//! CAP-67 Soroban events, shaped like the two sources this mirrors: stellar-go's
//! `LedgerTransaction.GetTransactionEvents` for the containers, and
//! stellar-rpc's `InsertEvents` for the ids (ADR 0059).

use serde_json::{Value, json};
use stellar_xdr::{
    ContractEvent, ContractEventBody, ContractEventType, ScAddress, TransactionEvent,
    TransactionEventStage, TransactionMeta,
};

use crate::scval::scval_to_typed_json;
use crate::types::{DiagnosticEvent, EventOrigin, ExtractedEvent};
use domain::ContractEventType as DomainEventType;

/// stellar-rpc's identity for a consensus event (ADR 0059): a TOID (SEP-35
/// layout) plus an event number. Source: stellar-rpc `internal/db/event.go`.
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

/// One transaction's events.
#[derive(Debug, Clone, Default)]
pub struct TxEvents {
    /// Consensus events, each with its id: the transaction-level ones, then
    /// each operation's, as stellar-rpc reads them. Id order is execution
    /// order.
    pub events: Vec<ExtractedEvent>,
    /// The host debug channel, in container order.
    pub diagnostic: Vec<DiagnosticEvent>,
}

/// One ledger's events, a transaction at a time — the only way to get them.
/// Transaction-level events are numbered across the whole ledger, so the
/// ledger is the unit; nothing is decoded until a transaction is asked for.
pub struct LedgerEvents<'a> {
    ledger_sequence: u32,
    created_at: i64,
    tx_metas: &'a [&'a TransactionMeta],
    /// stellar-rpc's ledger-wide `(beforeAll, afterAll)` counters as each
    /// transaction starts.
    starts: Vec<(u32, u32)>,
}

impl<'a> LedgerEvents<'a> {
    /// `tx_metas` is every transaction of the ledger, in apply order — also
    /// the ones a caller skips, because their fee events still count.
    pub fn new(ledger_sequence: u32, created_at: i64, tx_metas: &'a [&'a TransactionMeta]) -> Self {
        let (mut before_all, mut after_all) = (0, 0);
        let starts = tx_metas
            .iter()
            .map(|meta| {
                let start = (before_all, after_all);
                for event in containers(meta).transaction {
                    match event.stage {
                        TransactionEventStage::BeforeAllTxs => before_all += 1,
                        TransactionEventStage::AfterAllTxs => after_all += 1,
                        TransactionEventStage::AfterTx => {}
                    }
                }
                start
            })
            .collect();
        Self {
            ledger_sequence,
            created_at,
            tx_metas,
            starts,
        }
    }

    /// The events of the transaction at apply position `tx_index` (0-based):
    /// stellar-rpc's `InsertEvents` loop body. Empty when the ledger has no
    /// such transaction.
    pub fn extract(&self, tx_index: usize, transaction_hash: &str) -> TxEvents {
        let Some(meta) = self.tx_metas.get(tx_index) else {
            return TxEvents::default();
        };
        let containers = containers(meta);
        let application_order =
            u32::try_from(tx_index + 1).expect("transactions per ledger fit u32");
        let id = |transaction_index, operation_index, event_index| EventId {
            ledger_sequence: self.ledger_sequence,
            transaction_index,
            operation_index,
            event_index,
        };
        let consensus = |event_id, origin, event: &ContractEvent| {
            let (event_type, contract_id, topics, data) = decode(event);
            ExtractedEvent {
                transaction_hash: transaction_hash.to_string(),
                event_id,
                origin,
                event_type,
                contract_id,
                topics,
                data,
                created_at: self.created_at,
            }
        };

        let (mut before_all, mut after_all) = self.starts[tx_index];
        let mut after_tx = 0;
        let mut events = Vec::new();
        for event in containers.transaction {
            let event_id = match event.stage {
                TransactionEventStage::BeforeAllTxs => {
                    id(EventId::BEFORE_ALL_TXS, 0, next(&mut before_all))
                }
                TransactionEventStage::AfterAllTxs => {
                    id(EventId::AFTER_ALL_TXS, 0, next(&mut after_all))
                }
                TransactionEventStage::AfterTx => id(
                    application_order,
                    EventId::AFTER_TX_OPERATION,
                    next(&mut after_tx),
                ),
            };
            events.push(consensus(
                event_id,
                EventOrigin::Transaction(event.stage),
                &event.event,
            ));
        }
        for (op, op_events) in containers.operations.iter().enumerate() {
            let op = u16::try_from(op).expect("operations per transaction fit u16");
            for (n, event) in op_events.iter().enumerate() {
                let n = u32::try_from(n).expect("events per operation fit u32");
                events.push(consensus(
                    id(application_order, op, n),
                    EventOrigin::Operation(op),
                    event,
                ));
            }
        }

        let diagnostic = containers
            .diagnostic
            .iter()
            .map(|d| {
                let (event_type, contract_id, topics, data) = decode(&d.event);
                DiagnosticEvent {
                    event_type,
                    contract_id,
                    topics,
                    data,
                }
            })
            .collect();
        TxEvents { events, diagnostic }
    }
}

/// The counter's value, and the counter moved on.
fn next(counter: &mut u32) -> u32 {
    let n = *counter;
    *counter += 1;
    n
}

/// One transaction's three CAP-67 containers.
#[derive(Default)]
struct Containers<'a> {
    transaction: &'a [TransactionEvent],
    operations: Vec<&'a [ContractEvent]>,
    diagnostic: &'a [stellar_xdr::DiagnosticEvent],
}

/// stellar-go's `GetTransactionEvents`: V4 as it is; V3 holds a Soroban
/// transaction's single operation and no transaction-level events; earlier
/// metas hold no events.
fn containers(meta: &TransactionMeta) -> Containers<'_> {
    match meta {
        TransactionMeta::V3(v3) => match &v3.soroban_meta {
            Some(soroban) => Containers {
                transaction: &[],
                operations: vec![&soroban.events[..]],
                diagnostic: &soroban.diagnostic_events[..],
            },
            None => Containers::default(),
        },
        TransactionMeta::V4(v4) => Containers {
            transaction: &v4.events[..],
            operations: v4.operations.iter().map(|op| &op.events[..]).collect(),
            diagnostic: &v4.diagnostic_events[..],
        },
        _ => Containers::default(),
    }
}

/// An event's type, contract and ScVal-decoded topics and data.
fn decode(event: &ContractEvent) -> (DomainEventType, Option<String>, Value, Value) {
    // ADR 0031: the typed enum; persist binds it as SMALLINT.
    let event_type = match event.type_ {
        ContractEventType::System => DomainEventType::System,
        ContractEventType::Contract => DomainEventType::Contract,
        ContractEventType::Diagnostic => DomainEventType::Diagnostic,
    };
    let contract_id = event
        .contract_id
        .as_ref()
        .map(|id| ScAddress::Contract(id.clone()).to_string());
    let ContractEventBody::V0(body) = &event.body;
    let topics: Vec<Value> = body.topics.iter().map(scval_to_typed_json).collect();
    (
        event_type,
        contract_id,
        json!(topics),
        scval_to_typed_json(&body.data),
    )
}

#[cfg(test)]
mod tests;
