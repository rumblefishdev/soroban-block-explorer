use std::collections::HashMap;

use domain::ContractEventType;
use xdr_parser::types::{EventBody, EventOrigin, ExtractedEvent, ExtractedLedger};

use super::rows;
use crate::persist::stage::StagedLedger;

const CONTRACT: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

fn ledger() -> ExtractedLedger {
    ExtractedLedger {
        sequence: 10,
        hash: "00".repeat(32),
        closed_at: 1_700_000_000,
        protocol_version: 23,
        transaction_count: 1,
        base_fee: 100,
    }
}

fn event(event_index: u32) -> ExtractedEvent {
    ExtractedEvent {
        transaction_hash: "tx".into(),
        event_id: xdr_parser::EventId {
            ledger_sequence: 10,
            transaction_index: 1,
            operation_index: 0,
            event_index,
        },
        origin: EventOrigin::Operation(0),
        body: EventBody {
            event_type: ContractEventType::Contract,
            contract_id: Some(CONTRACT.into()),
            topics: serde_json::json!([{"type": "sym", "value": "transfer"}]),
            data: serde_json::json!({}),
        },
        created_at: 1_700_000_000,
    }
}

fn stage(events: Vec<ExtractedEvent>) -> Result<StagedLedger, crate::SchemaError> {
    let mut out = StagedLedger::default();
    let positions = HashMap::from([("tx".to_string(), 1)]);
    rows(
        &mut out,
        &[("tx".to_string(), events)],
        &positions,
        &ledger(),
        10,
    )?;
    Ok(out)
}

#[test]
fn distinct_ids_are_staged() {
    let out = stage(vec![event(0), event(1)]).expect("two ids");
    assert_eq!(out.event_rows.len(), 2);
}

#[test]
fn a_repeated_id_stops_the_ledger() {
    let err = stage(vec![event(0), event(0)]).expect_err("one id, two events");
    assert!(
        err.to_string().contains("0000000042949677056-0000000000"),
        "{err}"
    );
}
