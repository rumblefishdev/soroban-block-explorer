use super::*;

// Shape-valid StrKeys (56 chars, correct prefix, base32) — never minted on
// mainnet, used purely to exercise the parser's prefix/shape branches.
const C_STRKEY: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";
const G_STRKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";

fn asset_row(
    asset_type: i16,
    asset_code: Option<&str>,
    issuer: Option<&str>,
    contract_id: Option<&str>,
) -> AssetRow {
    AssetRow {
        asset_type,
        asset_type_name: None,
        asset_code: asset_code.map(String::from),
        issuer: issuer.map(String::from),
        contract_id: contract_id.map(String::from),
        name: None,
        symbol: None,
        decimals: 7,
        total_supply: None,
        holder_count: None,
        icon_url: None,
        deployed_at_ledger: None,
        issuer_home_domain: None,
        contract_surrogate_id: 0,
        sac_contract_surrogate: 0,
        sac_deployed: false,
        id: 0,
    }
}

#[test]
fn list_cursor_carries_the_seek_rank_not_the_hydrated_count() {
    // Task 0559: the page is selected by one query and hydrated by
    // another; a `balance_aggregates` rebuild in between changes
    // `holder_count`. The cursor must resume where THIS page ended.
    let mut row = asset_row(1, Some("USDC"), Some(G_STRKEY), None);
    row.id = 42;
    row.holder_count = Some(2);
    let listed = ListedAsset {
        row,
        holder_rank: 1,
    };

    let (dir, decoded) =
        cursor::decode::<AssetKeyCursor>(&listed_asset_cursor(Direction::Next, &listed))
            .expect("the list cursor decodes");

    assert_eq!(dir, Direction::Next);
    assert_eq!((decoded.holder_rank, decoded.id), (1, 42));
}

fn parsed_label(parsed: &Option<AssetIdRef<'_>>) -> &'static str {
    match parsed {
        None => "None",
        Some(AssetIdRef::Native) => "Native",
        Some(AssetIdRef::Contract(_)) => "Contract",
        Some(AssetIdRef::CodeIssuer(..)) => "CodeIssuer",
    }
}

// -- parse_asset_id ------------------------------------------------------

#[test]
fn parse_native_token_case_insensitive() {
    for raw in ["native", "NATIVE", "Native", "nAtIvE"] {
        assert!(
            matches!(parse_asset_id(raw), Some(AssetIdRef::Native)),
            "{raw:?} should parse as Native"
        );
    }
}

#[test]
fn parse_contract_strkey() {
    assert!(matches!(
        parse_asset_id(C_STRKEY),
        Some(AssetIdRef::Contract(c)) if c == C_STRKEY
    ));
}

#[test]
fn parse_code_issuer_composite() {
    let raw = format!("USDC-{G_STRKEY}");
    match parse_asset_id(&raw) {
        Some(AssetIdRef::CodeIssuer(code, issuer)) => {
            assert_eq!(code, "USDC");
            assert_eq!(issuer, G_STRKEY);
        }
        other => panic!("expected CodeIssuer, got {}", parsed_label(&other)),
    }
}

#[test]
fn parse_code_issuer_splits_on_last_dash() {
    // A code is never supposed to contain `-`, but if a hyphenated string
    // arrives the split is on the LAST dash so the issuer half validates.
    let raw = format!("WEIRD-CODE-{G_STRKEY}");
    match parse_asset_id(&raw) {
        Some(AssetIdRef::CodeIssuer(code, issuer)) => {
            assert_eq!(code, "WEIRD-CODE");
            assert_eq!(issuer, G_STRKEY);
        }
        other => panic!("expected CodeIssuer, got {}", parsed_label(&other)),
    }
}

#[test]
fn parse_rejects_numeric_id() {
    // The dropped numeric surrogate — must NOT resolve (handler → 400).
    for raw in ["12345", "0", "2147483647", "-1"] {
        assert!(
            parse_asset_id(raw).is_none(),
            "{raw:?} (numeric) must not parse"
        );
    }
}

#[test]
fn parse_rejects_malformed() {
    for raw in [
        "",                     // empty
        "not-an-asset-id",      // dash, but `id` is not a G-StrKey
        "USDC-",                // trailing dash → issuer empty
        "-GAAA",                // leading dash → idx == 0
        G_STRKEY,               // bare G-StrKey (not C, no dash)
        "USDC-NOTAVALIDISSUER", // issuer half wrong shape
        "CAAA",                 // too-short C prefix, no dash
        "nativex",              // close-but-not `native`, no dash, not C
    ] {
        assert!(parse_asset_id(raw).is_none(), "{raw:?} must not parse");
    }
}

#[test]
fn parse_code_issuer_requires_g_prefix_issuer() {
    // A C-StrKey in the issuer position is rejected (only G addresses issue).
    let raw = format!("USDC-{C_STRKEY}");
    assert!(parse_asset_id(&raw).is_none());
}

// -- canonical_id (wire token) ------------------------------------------

#[test]
fn canonical_id_prefers_contract_strkey() {
    // Defensive precedence: a present key `contract_id` wins over CODE-ISSUER.
    // Post-ADR 0051 a row never carries both (soroban has only a contract id,
    // classic only code+issuer), but the ordering is still the contract.
    let row = asset_row(3, Some("USDC"), Some(G_STRKEY), Some(C_STRKEY));
    assert_eq!(canonical_id(&row), C_STRKEY);
}

#[test]
fn map_item_rederives_sac_strkey_for_classic_wrap() {
    // ADR 0051: a classic_credit asset with an observed SAC surfaces the
    // re-derived C… StrKey (never stored) + the deployed flag. USDC's mainnet
    // SAC is a published constant — regression-guards the read-side derivation.
    const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let net = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);

    let mut row = asset_row(1, Some("USDC"), Some(USDC_ISSUER), None);
    row.sac_contract_surrogate = 42; // non-zero ⇒ "has SAC" (value irrelevant)
    row.sac_deployed = true;
    let item = map_item(row, &net);
    assert_eq!(
        item.sac_contract_id.as_deref(),
        Some("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75")
    );
    assert_eq!(item.sac_deployed, Some(true));
    assert_eq!(item.contract_id, None); // key contract_id stays soroban-only

    // No observed SAC ⇒ no facet on the wire.
    let plain = asset_row(1, Some("USDC"), Some(USDC_ISSUER), None);
    let plain_item = map_item(plain, &net);
    assert_eq!(plain_item.sac_contract_id, None);
    assert_eq!(plain_item.sac_deployed, None);
}

#[test]
fn canonical_id_contract_only() {
    let row = asset_row(3, None, None, Some(C_STRKEY));
    assert_eq!(canonical_id(&row), C_STRKEY);
}

#[test]
fn canonical_id_code_issuer_for_classic() {
    let row = asset_row(1, Some("USDC"), Some(G_STRKEY), None);
    assert_eq!(canonical_id(&row), format!("USDC-{G_STRKEY}"));
}

#[test]
fn canonical_id_native_singleton() {
    let row = asset_row(0, None, None, None);
    assert_eq!(canonical_id(&row), "native");
}

#[test]
fn canonical_id_empty_fallback_is_unreachable_but_safe() {
    // Defensive last arm: a non-native asset with neither a contract id nor
    // a full code+issuer pair violates `ck_assets_identity` (unreachable in
    // real data), but the token builder degrades to "" rather than panic.
    let code_only = asset_row(1, Some("USDC"), None, None);
    assert_eq!(canonical_id(&code_only), "");
    let issuer_only = asset_row(1, None, Some(G_STRKEY), None);
    assert_eq!(canonical_id(&issuer_only), "");
}

#[test]
fn canonical_id_roundtrips_through_parse() {
    // Every token canonical_id emits must parse back to the same form.
    let contract = asset_row(2, None, None, Some(C_STRKEY));
    assert!(matches!(
        parse_asset_id(&canonical_id(&contract)),
        Some(AssetIdRef::Contract(_))
    ));

    let classic = asset_row(1, Some("USDC"), Some(G_STRKEY), None);
    assert!(matches!(
        parse_asset_id(&canonical_id(&classic)),
        Some(AssetIdRef::CodeIssuer(..))
    ));

    let native = asset_row(0, None, None, None);
    assert!(matches!(
        parse_asset_id(&canonical_id(&native)),
        Some(AssetIdRef::Native)
    ));
}
