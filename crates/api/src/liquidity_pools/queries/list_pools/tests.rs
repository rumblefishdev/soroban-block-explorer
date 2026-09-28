use super::*;

#[test]
fn hex_pool_id_validation() {
    assert!(is_hex_pool_id(&"a".repeat(64)));
    assert!(is_hex_pool_id(&"0123456789abcdef".repeat(4)));
    assert!(!is_hex_pool_id(&"a".repeat(63)));
    assert!(!is_hex_pool_id(&"a".repeat(65)));
    assert!(!is_hex_pool_id(&"A".repeat(64)), "uppercase rejected");
    assert!(!is_hex_pool_id("xyz"));
    assert!(!is_hex_pool_id(&"'; DROP--".repeat(8)));
}
