use super::*;

#[test]
fn parses_a_list_of_targetable_tables_in_order() {
    let t =
        TargetedTables::parse("asset_transfers, transaction_memos,pool_operation_amounts").unwrap();
    assert_eq!(
        t.iter().collect::<Vec<_>>(),
        vec![
            "asset_transfers",
            "transaction_memos",
            "pool_operation_amounts"
        ]
    );
}

#[test]
fn soroban_event_ops_is_no_longer_targetable() {
    // Task 0541: the table is gone; the event's operation lives in the
    // `soroban_events` key.
    assert!(TargetedTables::parse("soroban_event_ops").is_err());
}

#[test]
fn the_old_single_table_form_still_parses() {
    let t = TargetedTables::parse("pool_operation_amounts").unwrap();
    assert_eq!(t.iter().collect::<Vec<_>>(), vec!["pool_operation_amounts"]);
}

#[test]
fn rejects_tables_that_are_not_additive() {
    // A Tier-1 table must never be targetable: a partial re-emission of it
    // would re-arm the `repair-tier1` obligation silently.
    let err = TargetedTables::parse("asset_transfers,transactions").unwrap_err();
    assert!(
        err.contains("`transactions` is not a targetable table"),
        "{err}"
    );
}

#[test]
fn rejects_duplicates_and_empty_lists() {
    assert!(TargetedTables::parse("asset_transfers,asset_transfers").is_err());
    assert!(TargetedTables::parse("").is_err());
    assert!(TargetedTables::parse(" , ").is_err());
}
