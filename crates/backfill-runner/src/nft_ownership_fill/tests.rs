use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::NftOwnershipChangeRow;

use super::{EventRow, changes};

const NFT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const UNKNOWN: &str = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const FUNGIBLE: &str = "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";
const ALICE: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const BOB: &str = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

fn event(
    contract: &str,
    contract_type: Option<i16>,
    (operation_index, event_index): (u16, u32),
    topics: &str,
    data: &str,
) -> EventRow {
    EventRow {
        contract: contract.into(),
        contract_type,
        ledger_sequence: 60_000_000,
        // The rpc id's transaction index differs from the stored position on
        // purpose: the position must come from `application_order`.
        transaction_index: 99,
        operation_index,
        event_index,
        application_order: 4,
        topics_xdr: topics.into(),
        data_xdr: data.into(),
    }
}

fn topics(name: &str, addrs: &[&str]) -> String {
    let mut t = vec![format!(r#"{{"type":"sym","value":"{name}"}}"#)];
    t.extend(
        addrs
            .iter()
            .map(|a| format!(r#"{{"type":"address","value":"{a}"}}"#)),
    );
    format!("[{}]", t.join(","))
}

fn change(
    contract: &str,
    token: &str,
    (op, ev): (u16, u32),
    owner: &str,
    event_type: i16,
) -> NftOwnershipChangeRow {
    NftOwnershipChangeRow {
        contract_id: ids::contract_id(contract),
        token_id: token.into(),
        ledger_sequence: 60_000_000,
        application_order: 4,
        operation_index: op,
        event_index: ev,
        owner_id: Some(ids::account_id(owner)),
        event_type,
    }
}

/// The indexer's extraction, run on stored events: each change located by its
/// event, one `consecutive_mint` under one id, routed by the contract's
/// current verdict.
#[test]
fn stored_events_become_located_changes_routed_by_verdict() {
    let events = [
        event(
            NFT,
            Some(2),
            (0, 0),
            &topics("consecutive_mint", &[ALICE]),
            r#"{"type":"vec","value":[{"type":"u32","value":1},{"type":"u32","value":2}]}"#,
        ),
        event(
            NFT,
            Some(2),
            (1, 0),
            &topics("transfer", &[ALICE, BOB]),
            r#"{"type":"u32","value":2}"#,
        ),
        event(
            UNKNOWN,
            None,
            (2, 0),
            &topics("mint", &[BOB]),
            r#"{"type":"u32","value":7}"#,
        ),
        event(
            FUNGIBLE,
            Some(3),
            (3, 0),
            &topics("mint", &[BOB]),
            r#"{"type":"u32","value":8}"#,
        ),
    ];

    let (hot, pending, dropped) = changes(&events).expect("changes");

    let mut expected_hot = vec![
        change(NFT, "1", (0, 0), ALICE, 0),
        change(NFT, "2", (0, 0), ALICE, 0),
        change(NFT, "2", (1, 0), BOB, 1),
    ];
    expected_hot.sort();
    assert_eq!(hot, expected_hot);
    assert_eq!(pending, vec![change(UNKNOWN, "7", (2, 0), BOB, 0)]);
    assert_eq!(dropped, 1);
}

/// A stored event whose JSON does not parse stops the fill instead of
/// silently losing a change.
#[test]
fn unparsable_stored_json_is_an_error() {
    let events = [event(NFT, Some(2), (0, 0), "not json", "{}")];
    assert!(changes(&events).is_err());
}

/// The gate counts a change once per copy, on each side it is missing from.
#[test]
fn diff_is_a_multiset_difference() {
    let c = |ledger| (1_i64, "t".to_string(), ledger, Some(9_i64), 1_i16);
    let (only_old, only_new) = super::diff(vec![c(1), c(1), c(2)], vec![c(1), c(3)]);
    assert_eq!(only_old, vec![c(1), c(2)]);
    assert_eq!(only_new, vec![c(3)]);
}
