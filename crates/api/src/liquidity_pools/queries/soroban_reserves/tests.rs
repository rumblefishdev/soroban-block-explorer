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

/// A leg gets a value only when its scale is a fact: 7 for native and classic
/// credit, the published `decimals` for a soroban token. A token with none
/// and an asset the dimension does not know stay `None`, never a raw integer.
#[test]
fn legs_scale_by_their_known_decimals_only() {
    let mut token = asset(domain::AssetFamily::Soroban as i16, true);
    token.contract_strkey = Some("CTOKEN18".to_string());
    let mut unpublished = asset(domain::AssetFamily::Soroban as i16, true);
    unpublished.contract_strkey = Some("CNOMETA".to_string());
    let identities = HashMap::from([
        (1, asset(domain::AssetFamily::Native as i16, true)),
        (2, asset(domain::AssetFamily::ClassicCredit as i16, true)),
        (3, token),
        (4, unpublished),
        (5, asset(domain::AssetFamily::ClassicCredit as i16, false)),
    ]);
    let decimals = HashMap::from([("CTOKEN18".to_string(), 18)]);
    let raw: Vec<String> = ["10", "20", "1282501540990846914271528", "40", "50"]
        .map(String::from)
        .to_vec();
    assert_eq!(
        leg_reserves(&[1, 2, 3, 4, 5, 6], &identities, &decimals, &raw),
        vec![
            Some("0.000001".to_string()),
            Some("0.000002".to_string()),
            Some("1282501.540990846914271528".to_string()),
            None, // soroban token that publishes no decimals
            None, // unknown to the dimension
            None, // no identity AND no raw value
        ]
    );
}

#[test]
fn soroban_token_contracts_are_the_known_soroban_legs_deduplicated() {
    let mut a = asset(domain::AssetFamily::Soroban as i16, true);
    a.contract_strkey = Some("CA".to_string());
    let mut b = asset(domain::AssetFamily::Soroban as i16, false);
    b.contract_strkey = Some("CB".to_string());
    let identities = HashMap::from([
        (1, a.clone()),
        (2, a),
        (3, b),
        (4, asset(domain::AssetFamily::Native as i16, true)),
    ]);
    assert_eq!(
        soroban_token_contracts(&[1, 2, 3, 4], &identities),
        vec!["CA"]
    );
}
