use super::*;

fn observed(num_sponsoring: Option<u32>, num_sponsored: Option<u32>) -> ExtractedAccountState {
    ExtractedAccountState {
        account_id: "GWALLET".to_string(),
        first_seen_ledger: None,
        last_seen_ledger: 64_800_104,
        sequence_number: 7,
        balances: serde_json::json!([]),
        removed_trustlines: vec![],
        account_removed: false,
        signers: Some(vec![]),
        thresholds: Some("01000000".to_string()),
        flags: Some(0),
        num_sponsoring,
        num_sponsored,
        home_domain: None,
        created_at: 1_700_000_000,
    }
}

/// The row stores the counters the entry carried. Values from mainnet
/// (`getLedgerEntries`, 2026-10-06). lore-0629.
#[test]
fn row_carries_the_sponsorship_counters() {
    let row = entry_state_row(
        &observed(Some(4_044_091), Some(0)),
        "01000000",
        1,
        64_800_104,
    )
    .expect("well-formed thresholds give a row");
    assert_eq!((row.num_sponsoring, row.num_sponsored), (4_044_091, 0));

    let row = entry_state_row(&observed(Some(0), Some(3)), "01000000", 2, 64_800_104).unwrap();
    assert_eq!((row.num_sponsoring, row.num_sponsored), (0, 3));
}
