use super::*;

#[test]
fn asset_family_name_matches_pg_function() {
    assert_eq!(asset_family_name(0).as_deref(), Some("native"));
    assert_eq!(asset_family_name(1).as_deref(), Some("classic_credit"));
    // 2 (`sac`) retired — ADR 0051.
    assert_eq!(asset_family_name(2), None);
    assert_eq!(asset_family_name(3).as_deref(), Some("soroban"));
    assert_eq!(asset_family_name(99), None);
}

#[test]
fn route_token_prefers_contract_strkey() {
    assert_eq!(
        asset_route_token(Some("CABC"), Some("USDC"), Some("GISS"), 1).as_deref(),
        Some("CABC"),
    );
}

#[test]
fn route_token_falls_back_to_code_issuer() {
    assert_eq!(
        asset_route_token(None, Some("USDC"), Some("GISS"), 1).as_deref(),
        Some("USDC-GISS"),
    );
    // Empty contract strkey is treated as absent (nullIf sentinel parity).
    assert_eq!(
        asset_route_token(Some(""), Some("USDC"), Some("GISS"), 1).as_deref(),
        Some("USDC-GISS"),
    );
}

#[test]
fn route_token_native_only_when_type_zero() {
    assert_eq!(
        asset_route_token(None, None, None, 0).as_deref(),
        Some("native"),
    );
    // Code present but issuer unresolved on a non-native row → honest None
    // (never a mis-route to the native page).
    assert_eq!(asset_route_token(None, Some("USDC"), None, 1), None);
}
