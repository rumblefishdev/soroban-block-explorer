use super::*;

#[test]
fn event_appearances_filter_on_the_transaction_position() {
    let sql = event_appearances_sql();
    assert!(sql.contains("se.ledger_sequence = ?"));
    assert!(sql.contains("se.application_order = ?"));
    assert!(!sql.contains("transaction_id"));
    assert!(!sql.contains("FINAL"));
}
