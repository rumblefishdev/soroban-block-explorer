use super::*;

#[test]
fn decimal_str_validation() {
    assert!(is_decimal_str("0"));
    assert!(is_decimal_str("123.4567890"));
    assert!(is_decimal_str("-5.5"));
    assert!(!is_decimal_str(""));
    assert!(!is_decimal_str("1.2.3"));
    assert!(!is_decimal_str("1e9"));
    assert!(!is_decimal_str("'; DROP"));
    assert!(!is_decimal_str("abc"));
}
