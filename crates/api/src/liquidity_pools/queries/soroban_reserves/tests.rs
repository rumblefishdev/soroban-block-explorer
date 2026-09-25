use super::*;

fn asset(asset_type: i16, known: bool) -> ResolvedAsset {
    ResolvedAsset {
        known,
        asset_type,
        asset_code: None,
        issuer: None,
        contract_strkey: None,
        symbol: None,
        decimals: 7,
    }
}

#[test]
fn stroops_scale_like_a_classic_decimal() {
    assert_eq!(
        scale_by_7("31072879007206").as_deref(),
        Some("3107287.9007206")
    );
    assert_eq!(scale_by_7("10000000").as_deref(), Some("1"));
    assert_eq!(scale_by_7("125000000").as_deref(), Some("12.5"));
    assert_eq!(scale_by_7("1").as_deref(), Some("0.0000001"));
    assert_eq!(scale_by_7("0").as_deref(), Some("0"));
    assert_eq!(scale_by_7("-1").as_deref(), Some("-0.0000001"));
    // An 18-decimal stable reserve still parses as i128.
    assert_eq!(
        scale_by_7("1282501540990846914271528").as_deref(),
        Some("128250154099084691.4271528")
    );
    assert_eq!(scale_by_7("not a number"), None);
}

/// Only a leg whose scale is a protocol fact gets a value: native and classic
/// credit. A soroban token (its decimals live in metadata) and an asset the
/// dimension does not know stay `None` rather than a raw integer.
#[test]
fn only_native_and_classic_legs_are_served() {
    let identities = HashMap::from([
        (1, asset(domain::AssetFamily::Native as i16, true)),
        (2, asset(domain::AssetFamily::ClassicCredit as i16, true)),
        (3, asset(domain::AssetFamily::Soroban as i16, true)),
        (4, asset(domain::AssetFamily::ClassicCredit as i16, false)),
    ]);
    let raw: Vec<String> = ["10", "20", "30", "40"].map(String::from).to_vec();
    assert_eq!(
        leg_reserves(&[1, 2, 3, 4, 5], &identities, &raw),
        vec![
            Some("0.000001".to_string()),
            Some("0.000002".to_string()),
            None,
            None,
            None, // leg 5: unknown to the dimension AND no raw value
        ]
    );
}
