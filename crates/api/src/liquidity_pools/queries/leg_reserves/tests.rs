use super::*;

/// A classic snapshot is already in units and pair-shaped.
#[test]
fn a_snapshot_pair_is_used_verbatim() {
    let r = Reserves::Pair(Some("750.5"), None);
    assert_eq!(r.at(0, None).as_deref(), Some("750.5"));
    assert_eq!(r.at(1, Some(7)), None);
    assert_eq!(r.at(2, Some(7)), None, "a third leg has no slot in a pair");
}

/// Each soroban leg scales by its own decimals, and an unknown scale yields
/// nothing rather than a guessed-7 number wrong by up to 10^11.
#[test]
fn raw_reserves_scale_per_leg() {
    let raw = vec![
        "12345678".to_string(),
        "5000000000000000000".to_string(),
        "0".to_string(),
        "77".to_string(),
    ];
    let r = Reserves::Raw(&raw);
    assert_eq!(r.at(0, Some(7)).as_deref(), Some("1.2345678"));
    assert_eq!(r.at(1, Some(18)).as_deref(), Some("5"));
    // Zero needs no scale.
    assert_eq!(r.at(2, None).as_deref(), Some("0"));
    assert_eq!(r.at(3, None), None, "no known scale, no value");
    assert_eq!(r.at(4, Some(7)), None, "a leg the vector does not reach");
}
