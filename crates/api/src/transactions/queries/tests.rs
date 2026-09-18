use super::*;

#[test]
fn contract_page_seeks_and_orders_by_position() {
    let sql = contract_page_sql(
        &[(64_000_001, 3), (64_000_000, 12)],
        "NULL",
        "NULL",
        "DESC",
        21,
    );
    assert!(
        sql.contains("(t.ledger_sequence, t.application_order) IN ((64000001,3),(64000000,12))")
    );
    assert!(sql.contains("intDiv(t.ledger_sequence, 500000) IN (128)"));
    assert!(sql.contains("ORDER BY t.ledger_sequence DESC, t.application_order DESC"));
    assert!(sql.contains("LIMIT 1 BY t.ledger_sequence, t.application_order"));
}

#[test]
fn event_appearances_filter_on_the_transaction_position() {
    let sql = event_appearances_sql();
    assert!(sql.contains("se.ledger_sequence = ?"));
    assert!(sql.contains("se.application_order = ?"));
    assert!(!sql.contains("transaction_id"));
    assert!(!sql.contains("FINAL"));
}

#[test]
fn page_row_merges_aggregates_and_maps_sentinels() {
    // Slim page row: empty-string sentinels → None, millis → UTC, and the
    // separately-fetched aggregate (op types) merges in by id. Replaces the
    // old correlated-projection mapping.
    let row = TxPageChRow {
        hash: "ab".repeat(32),
        ledger_sequence: 100,
        application_order: 2,
        source_account: Some(String::new()),
        fee_charged: 100,
        inner_tx_hash: None,
        successful: true,
        operation_count: 1,
        has_soroban: false,
        id: 999,
        created_at: 1_700_000_000_000,
    };
    let agg = ch::TxListAggregates {
        operation_types: vec!["CREATE_ACCOUNT".to_string(), "PAYMENT".to_string()],
    };
    let mapped = row.into_list_row(agg);
    assert_eq!(mapped.source_account, None);
    assert_eq!(mapped.inner_tx_hash, None);
    assert_eq!(mapped.id, 999);
    assert_eq!(mapped.ledger_sequence, 100);
    assert_eq!(
        mapped.operation_types,
        vec!["CREATE_ACCOUNT".to_string(), "PAYMENT".to_string()],
    );
    assert_eq!(mapped.created_at, ch::millis_to_utc(1_700_000_000_000));
}
