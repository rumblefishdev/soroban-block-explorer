use super::*;

/// A classic pool has no state change: its snapshot pair is used as it is,
/// already in units, and a third leg has no slot in it.
#[test]
fn a_classic_pool_reads_its_snapshot_pair() {
    let pair = [Some("750.5"), None];
    assert_eq!(leg_reserve(0, &[], pair, None).as_deref(), Some("750.5"));
    assert_eq!(leg_reserve(1, &[], pair, Some(7)), None);
    assert_eq!(leg_reserve(2, &[], pair, Some(7)), None);
}

/// Each soroban leg scales by its own decimals, and an unknown scale yields
/// nothing rather than a guessed-7 number wrong by 10^11.
#[test]
fn raw_reserves_scale_per_leg() {
    let state: Vec<String> = ["12345678", "5000000000000000000", "0", "77"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let none = [None, None];
    assert_eq!(
        leg_reserve(0, &state, none, Some(7)).as_deref(),
        Some("1.2345678")
    );
    assert_eq!(leg_reserve(1, &state, none, Some(18)).as_deref(), Some("5"));
    // Zero needs no scale.
    assert_eq!(leg_reserve(2, &state, none, None).as_deref(), Some("0"));
    assert_eq!(
        leg_reserve(3, &state, none, None),
        None,
        "no known scale, no value"
    );
    assert_eq!(
        leg_reserve(4, &state, none, Some(7)),
        None,
        "a leg the vector does not reach"
    );
}
