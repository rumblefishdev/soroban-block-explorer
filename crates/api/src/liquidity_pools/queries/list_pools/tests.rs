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

/// A soroban pool's TVL prices the SCALED reserves: raw 10000000 and 30000000
/// at 7 decimals are 1 and 3 units, so 1 × 0.5 + 3 × 1.0 = 3.5 — not the
/// 35,000,000 the raw integers would give.
#[test]
fn soroban_tvl_prices_scaled_reserves() {
    use crate::common::asset_identity::ResolvedAsset;
    use crate::liquidity_pools::queries::price_leg;

    const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let asset = |asset_type: i16, code: Option<&str>, issuer: Option<&str>| ResolvedAsset {
        known: true,
        asset_type,
        asset_code: code.map(str::to_string),
        issuer: issuer.map(str::to_string),
        contract_strkey: None,
        symbol: None,
        decimals: Some(7),
    };
    let identities = HashMap::from([
        (1001, asset(0, None, None)),
        (1002, asset(1, Some("USDC"), Some(ISSUER))),
    ]);
    let raw = ["10000000".to_string(), "30000000".to_string()];
    let legs = leg_rows(
        &[1001, 1002],
        &identities,
        &HashMap::new(),
        Reserves::Raw(&raw),
    );

    let xlm = price_leg(0, None, None);
    let usdc = price_leg(1, Some("USDC"), Some(ISSUER));
    let closes = HashMap::from([(xlm.clone(), 0.5), (usdc.clone(), 1.0)]);
    assert_eq!(legs_tvl(&legs, &[xlm, usdc], &closes), Some(usd_str(3.5)));
}
