use super::*;

#[test]
fn nft_event_type_name_matches_pg_function() {
    assert_eq!(nft_event_type_name(0).as_deref(), Some("mint"));
    assert_eq!(nft_event_type_name(1).as_deref(), Some("transfer"));
    assert_eq!(nft_event_type_name(2).as_deref(), Some("burn"));
    // Out-of-range → None, matching the PG CASE's NULL (no degrade label).
    assert_eq!(nft_event_type_name(3), None);
    assert_eq!(nft_event_type_name(-1), None);
}
