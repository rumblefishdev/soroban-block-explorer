use super::*;

fn asset(asset_type: i16, known: bool) -> ResolvedAsset {
    ResolvedAsset {
        known,
        asset_type,
        asset_code: None,
        issuer: None,
        contract_strkey: None,
        symbol: None,
        decimals: Some(7),
    }
}

#[test]
fn stroops_scale_like_a_classic_decimal() {
    assert_eq!(
        scale_raw("31072879007206", 7).as_deref(),
        Some("3107287.9007206")
    );
    assert_eq!(scale_raw("10000000", 7).as_deref(), Some("1"));
    assert_eq!(scale_raw("125000000", 7).as_deref(), Some("12.5"));
    assert_eq!(scale_raw("1", 7).as_deref(), Some("0.0000001"));
    assert_eq!(scale_raw("0", 7).as_deref(), Some("0"));
    assert_eq!(scale_raw("-1", 7).as_deref(), Some("-0.0000001"));
    // An 18-decimal stable reserve still parses as i128.
    assert_eq!(
        scale_raw("1282501540990846914271528", 7).as_deref(),
        Some("128250154099084691.4271528")
    );
    assert_eq!(scale_raw("not a number", 7), None);
}

/// A leg gets a value only when the resolver knows its scale, and its raw
/// value parses; never a raw integer.
#[test]
fn legs_scale_by_their_known_decimals_only() {
    let with = |d: Option<u32>| ResolvedAsset {
        decimals: d,
        ..asset(domain::AssetFamily::Soroban as i16, true)
    };
    let identities = HashMap::from([
        (1, asset(domain::AssetFamily::Native as i16, true)),
        (2, with(Some(18))),
        (3, with(None)),
    ]);
    let raw: Vec<String> = ["10", "1282501540990846914271528", "30"]
        .map(String::from)
        .to_vec();
    assert_eq!(
        leg_reserves(&[1, 2, 3, 4], &identities, &raw),
        vec![
            Some("0.000001".to_string()),
            Some("1282501.540990846914271528".to_string()),
            None, // a token that publishes no decimals
            None, // no identity AND no raw value
        ]
    );
}
