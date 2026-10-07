use super::*;

fn event_row(event_type: i16, topics_xdr: &str, data_xdr: &str) -> EventChRow {
    EventChRow {
        ledger_sequence: 100,
        transaction_index: 7,
        operation_index: 0,
        event_index: 2,
        event_type,
        topics_xdr: topics_xdr.to_string(),
        data_xdr: data_xdr.to_string(),
        transaction_hash: "deadbeef".to_string(),
        successful: true,
        created_at: 1_700_000_000_000,
    }
}

#[test]
fn map_event_row_decodes_payload_type_and_index() {
    let ev = map_event_row(event_row(
        1, // contract
        r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GABC"}]"#,
        r#"{"type":"i128","value":"1000"}"#,
    ))
    .expect("a valid row decodes");
    assert_eq!(ev.event_index, 2); // cursor part, carried beside the wire id
    assert_eq!(ev.item.event_type, "contract");
    assert_eq!(ev.item.topics.len(), 2); // JSON array → its elements
    assert_eq!(ev.item.data["value"], "1000");
    assert_eq!(ev.item.transaction_hash, "deadbeef");
    assert_eq!(ev.item.ledger_sequence, 100);
    assert!(ev.item.successful);
}

#[test]
fn map_event_row_exposes_the_rpc_id() {
    let mut row = event_row(1, "[]", "null");
    row.ledger_sequence = 64_450_000;
    row.transaction_index = 0;
    row.event_index = 0;
    let ev = map_event_row(row).expect("a valid row decodes");
    // Ledger 64,450,000's first fee charge as mainnet getEvents returns it.
    assert_eq!(ev.item.id, "0276810642227200000-0000000000");
    assert_eq!(
        (ev.transaction_index, ev.operation_index, ev.event_index),
        (0, 0, 0)
    );
}

#[test]
fn events_page_orders_and_seeks_on_the_rpc_id() {
    let cursor = EventCursor::ChEventId {
        ledger_sequence: 1,
        transaction_index: 2,
        operation_index: 3,
        event_index: 4,
    };
    let sql = events_page_sql(Some(&cursor), Direction::Next);
    assert!(sql.contains(
        "(se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index) < (1, 2, 3, 4)"
    ));
    assert!(sql.contains(
        "ORDER BY se.ledger_sequence DESC, se.transaction_index DESC, se.operation_index DESC, se.event_index DESC"
    ));
    assert!(!sql.contains("transaction_id"));
    assert!(!events_page_sql(None, Direction::Next).contains(") < ("));
}

/// `LIMIT 1 BY` walks every row it dedups, so it must run on the key columns
/// alone — with the payload beside it, the native SAC's page read 1.2 GiB.
#[test]
fn events_page_dedups_keys_before_reading_the_payload() {
    let sql = events_page_sql(None, Direction::Next);
    let (outer, inner) = sql.split_once(" IN ( ").expect("keys subquery");
    let (inner, _) = inner.split_once("LIMIT ?)").expect("subquery end");
    assert!(inner.starts_with("SELECT se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index FROM soroban_events se"));
    assert!(inner.contains("LIMIT 1 BY se.ledger_sequence"));
    for heavy in ["topics_xdr", "data_xdr"] {
        assert!(!inner.contains(heavy), "{heavy} read inside the dedup");
        assert!(outer.contains(heavy));
    }
    assert_eq!(sql.matches("contract_id = ?").count(), 2);
}

#[test]
fn event_transactions_resolve_by_position() {
    let sql = event_transactions_sql(&[(10, 1), (10, 3)]);
    assert!(sql.contains("(t.ledger_sequence, t.application_order) IN ((10,1),(10,3))"));
    assert!(!sql.contains("t.id"));
}

#[test]
fn map_event_row_scalar_topics_wraps_singleton() {
    let ev =
        map_event_row(event_row(0 /* system */, r#""solo""#, "null")).expect("a valid row decodes");
    assert_eq!(ev.item.event_type, "system");
    assert_eq!(ev.item.topics.len(), 1); // scalar JSON → singleton vec
    assert!(ev.item.data.is_null());
}

#[test]
fn map_event_row_event_type_labels_and_out_of_range() {
    let label = |t| {
        map_event_row(event_row(t, "[]", "null"))
            .expect("valid")
            .item
            .event_type
    };
    assert_eq!(label(0), "system");
    assert_eq!(label(1), "contract");
    assert_eq!(label(2), "diagnostic");
    // No such type exists; the row is broken, not "unknown".
    assert!(map_event_row(event_row(99, "[]", "null")).is_err());
}

#[test]
fn map_event_row_malformed_payload_is_an_error() {
    assert!(map_event_row(event_row(1, "not json", "null")).is_err());
    assert!(map_event_row(event_row(1, "[]", "also not json")).is_err());
}
