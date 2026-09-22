use super::*;

#[test]
fn contract_type_name_matches_pg_function() {
    assert_eq!(contract_type_name(0).as_deref(), Some("token"));
    assert_eq!(contract_type_name(1).as_deref(), Some("other"));
    assert_eq!(contract_type_name(2).as_deref(), Some("nft"));
    assert_eq!(contract_type_name(3).as_deref(), Some("fungible"));
    assert_eq!(contract_type_name(4), None);
}

#[test]
fn map_upgradeable_three_state() {
    // SAC → Immutable regardless of the join code: nothing to swap.
    assert_eq!(map_upgradeable(false, true, -1), Some(false));
    assert_eq!(map_upgradeable(false, true, 1), Some(false));
    // WASM present: 1 → upgradeable, 0 → frozen.
    assert_eq!(map_upgradeable(true, false, 1), Some(true));
    assert_eq!(map_upgradeable(true, false, 0), Some(false));
    // WASM present, -1 (no metadata row / pre-0327 key absent) → Unknown.
    assert_eq!(map_upgradeable(true, false, -1), None);
}

/// Task 0548 — the case that used to answer a confident "cannot upgrade"
/// about a contract we know nothing about. Covers both populations: a
/// pre-0548 placeholder row (no deploy observed) and, from protocol 28, a
/// contract whose code is owned by another contract.
#[test]
fn no_wasm_and_not_a_sac_is_unknown_not_immutable() {
    assert_eq!(map_upgradeable(false, false, -1), None);
    assert_eq!(
        map_upgradeable(false, false, 1),
        None,
        "an executable we never resolved cannot be reported as frozen or as \
         upgradeable — the chip must stay off"
    );
}

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
    ));
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
    let ev = map_event_row(row);
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
    let ev = map_event_row(event_row(0 /* system */, r#""solo""#, "null"));
    assert_eq!(ev.item.event_type, "system");
    assert_eq!(ev.item.topics.len(), 1); // scalar JSON → singleton vec
    assert!(ev.item.data.is_null());
}

#[test]
fn map_event_row_event_type_labels_and_out_of_range() {
    assert_eq!(
        map_event_row(event_row(0, "[]", "null")).item.event_type,
        "system"
    );
    assert_eq!(
        map_event_row(event_row(1, "[]", "null")).item.event_type,
        "contract"
    );
    assert_eq!(
        map_event_row(event_row(2, "[]", "null")).item.event_type,
        "diagnostic"
    );
    // Out-of-range discriminant → empty string (try_from fails, default).
    assert_eq!(
        map_event_row(event_row(99, "[]", "null")).item.event_type,
        ""
    );
}

#[test]
fn map_event_row_malformed_payload_degrades_not_drops() {
    let ev = map_event_row(event_row(1, "not json", "also not json"));
    assert!(ev.item.topics.is_empty()); // decode fail → empty, row still emitted
    assert!(ev.item.data.is_null());
}

/// The wire label must stay derived from the constant, so the number the
/// SQL windows on and the string the client is told can never disagree.
#[test]
fn wire_label_is_derived_from_the_window_constant() {
    assert_eq!(stats_window_label(), format!("{STATS_WINDOW_DAYS} days"));
    assert!(stats_window_label().starts_with(&STATS_WINDOW_DAYS.to_string()));
}

// Regression guard for task 0300: CH `recent_events` was hardcoded `0`.
// The stats SQL MUST select a real windowed event count off `soroban_events`
// (parity with PG's appearance-fold SUM), not a literal.
#[test]
fn stats_sql_computes_recent_events_from_events_table() {
    let sql = contract_stats_sql(7);

    assert!(
        sql.contains("AS recent_events"),
        "recent_events column missing: {sql}"
    );
    assert!(
        sql.contains("FROM soroban_events se"),
        "recent_events must read soroban_events: {sql}"
    );
    // The bug shape: a bare literal aliased to recent_events. Collapse
    // whitespace first so the guard is alignment-independent.
    let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !normalized.contains("0 AS recent_events"),
        "recent_events still hardcoded to a literal: {sql}"
    );
    // Window parity: both the invocations seek and the events subquery
    // apply the same ledger floor + INTERVAL N DAY bound.
    assert_eq!(
        sql.matches("INTERVAL 7 DAY").count(),
        2,
        "events window must mirror the invocations window: {sql}"
    );
    // Two binds: events-subquery contract_id, then outer contract_id.
    assert_eq!(
        sql.matches("contract_id = ?").count(),
        2,
        "expected two `contract_id = ?` binds: {sql}"
    );
    // The scalar subquery MUST be `ifNull(…, 0)`-wrapped: CH types a bare
    // `(SELECT …)` as Nullable(UInt64), which fails the non-nullable `u64`
    // decode → 500 on every contract detail.
    assert!(
        normalized.contains("ifNull(( SELECT toUInt64(count())"),
        "recent_events subquery must be ifNull-wrapped: {sql}"
    );
}

/// Regression guard for lore-0420. Two failure modes, one shape.
///
/// `ledgers` is a ReplacingMergeTree with unmerged duplicate rows, so
/// JOINing it into a `count()` multiplies the count by the number of
/// physical copies (measured ~1.6x). And the seek bound must be resolved
/// from the data, not from a hardcoded ledgers-per-day constant: the old
/// `days * 17_280` assumed a 5 s cadence, ran 13% wide against the real
/// ~5.6 s, and would silently run SHORT — under-reporting the window — if
/// the chain ever sped up.
///
/// One `min(sequence)` bound satisfies both: immune to duplicates, exact by
/// construction.
#[test]
fn stats_sql_bounds_window_from_data_never_a_join_or_a_constant() {
    let sql = contract_stats_sql(7);
    let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        !normalized.contains("JOIN ledgers"),
        "a JOIN onto ledgers fans each row out per duplicate copy and \
         inflates the count: {sql}"
    );
    assert!(
        !normalized.contains("17280") && !normalized.contains("17_280"),
        "the window bound must come from the data, not a ledgers-per-day \
         constant that drifts with the chain cadence: {sql}"
    );
    // One per window: the invocations seek and the events subquery.
    assert_eq!(
        normalized
            .matches("SELECT min(sequence) FROM ledgers WHERE closed_at >=")
            .count(),
        2,
        "both the invocations and events windows must derive their bound \
         from the data: {sql}"
    );
}
