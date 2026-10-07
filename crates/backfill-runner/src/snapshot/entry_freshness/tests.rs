use super::*;

#[test]
fn only_a_missing_or_older_snapshot_needs_an_insert() {
    assert_eq!(need(None, 64_000_000), Need::Missing);
    assert_eq!(need(Some(63_999_999), 64_000_000), Need::Stale);
    assert_eq!(need(Some(64_000_000), 64_000_000), Need::Current);
    // A `state` read stamps our snapshot with the reading ledger, later than
    // the entry's own last modification.
    assert_eq!(need(Some(64_000_050), 64_000_000), Need::Current);
}
