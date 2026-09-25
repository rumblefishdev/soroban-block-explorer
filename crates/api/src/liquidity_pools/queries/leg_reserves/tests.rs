use super::*;

/// A classic pool has no state change: its two snapshot columns are its legs.
#[test]
fn a_classic_pool_reads_its_snapshot_pair() {
    assert_eq!(
        leg_reserves(&[], Some("7506999160"), None),
        vec![Some("7506999160".to_string()), None]
    );
}

/// A soroban pool's state change wins, one raw value per leg, however many.
#[test]
fn a_soroban_pool_reads_its_state_change() {
    let state = vec!["1".to_string(), "2".to_string(), "3".to_string()];
    assert_eq!(
        leg_reserves(&state, Some("9"), Some("9")),
        vec![
            Some("1".to_string()),
            Some("2".to_string()),
            Some("3".to_string())
        ]
    );
}
