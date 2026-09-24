use super::*;

/// A classic snapshot is already scaled and must not be scaled again.
#[test]
fn a_snapshot_value_wins() {
    assert_eq!(
        total_shares_of(
            Some("750.699916".into()),
            Some("9516607233561"),
            Some(7),
            false
        ),
        Some("750.699916".to_string())
    );
}

#[test]
fn a_soroban_pool_reads_its_instance_state() {
    assert_eq!(
        total_shares_of(None, Some("252647541418"), Some(7), false),
        Some("25264.7541418".to_string())
    );
    // The share token publishes no decimals: no value, never a guessed 7.
    assert_eq!(
        total_shares_of(None, Some("252647541418"), None, false),
        None
    );
    assert_eq!(total_shares_of(None, None, Some(7), true), None);
}

/// A stored 0 is a number only when it is a measurement.
#[test]
fn a_zero_is_shown_only_when_measured() {
    assert_eq!(
        total_shares_of(None, Some("0"), Some(7), true),
        Some("0".to_string())
    );
    assert_eq!(total_shares_of(None, Some("0"), Some(7), false), None);
}

#[test]
fn zero_is_measured_for_the_pair_factory_and_for_an_empty_pool() {
    let empty = vec!["0".to_string(), "0".to_string()];
    let holding = vec!["10100000000".to_string(), "0".to_string()];
    // Pair-factory: the family with no type marker always stores the key.
    assert!(zero_shares_is_measured("", &holding));
    // An empty router pool has nothing outstanding, whatever its storage says.
    assert!(zero_shares_is_measured("constant", &empty));
    // A router pool holding reserves with a 0: the key was absent — unknown.
    assert!(!zero_shares_is_measured("constant", &holding));
    assert!(!zero_shares_is_measured("concentrated", &holding));
    // No reserve row at all is not evidence of an empty pool.
    assert!(!zero_shares_is_measured("stable", &[]));
}
