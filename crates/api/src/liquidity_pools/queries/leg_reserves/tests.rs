use super::*;

/// Real values off production, scaled by 7 decimals.
#[test]
fn scaling_inserts_the_point() {
    assert_eq!(
        scale_decimal_str("9516607233561", 7).unwrap(),
        "951660.7233561"
    );
    assert_eq!(scale_decimal_str("100000", 7).unwrap(), "0.01");
    assert_eq!(scale_decimal_str("1000000000000000000", 18).unwrap(), "1");
}

/// A `u128` past 2^53, where an `f64` round-trip would start dropping digits —
/// the reason this is string surgery and not arithmetic.
#[test]
fn scaling_is_exact_beyond_the_float_range() {
    assert_eq!(
        scale_decimal_str("340282366920938463463374607431768211455", 7).unwrap(),
        "34028236692093846346337460743176.8211455"
    );
}

#[test]
fn scaling_handles_the_edges() {
    // Fewer digits than the scale: left-padded, never a bare ".01".
    assert_eq!(scale_decimal_str("1", 7).unwrap(), "0.0000001");
    // No fractional part left once trailing zeros go.
    assert_eq!(scale_decimal_str("10000000", 7).unwrap(), "1");
    assert_eq!(scale_decimal_str("42", 0).unwrap(), "42");
    // Not a number: no value rather than a wrong one.
    assert_eq!(scale_decimal_str("", 7), None);
    assert_eq!(scale_decimal_str("-5", 7), None);
    assert_eq!(scale_decimal_str("12x4", 7), None);
    // Contract-published decimals past what a u128 can need are not a scale.
    assert_eq!(scale_decimal_str("1", 43_224), None);
}

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
