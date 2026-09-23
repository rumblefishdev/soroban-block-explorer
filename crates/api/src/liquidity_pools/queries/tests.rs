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

/// The pool-leg surrogates this module computes from `liquidity_pools`
/// columns MUST equal the ones the indexer writes into
/// `lp_operation_amounts.asset_id` from a claim atom's asset string
/// (`stage.rs::claim_atom_asset_id` → `ids::credit_asset_id` /
/// `NATIVE_ASSET_ID`). They meet only through this equality: if it breaks,
/// no row ever matches a leg and the Amount column silently goes blank
/// instead of failing. The bridge is `asset_a_issuer_id`, which the writer
/// fills with `ids::account_id(issuer_strkey)`.
///
/// Every XDR asset type a pool leg can hold is covered here on purpose.
/// The first version of this test used `"TF"` — `credit_alphanum4`, XDR
/// type 1 — and so agreed with the buggy resolution: type 2 is
/// `credit_alphanum12` in `liquidity_pools`, but the retired SAC facet in
/// `ids::asset_id`, which answered `0` for it. 59% of pools carry a type-2
/// leg and the suite stayed green (task 0489). A code of each width is now
/// pinned, so the next type-space mix-up fails here.
#[test]
fn pool_leg_surrogates_match_the_written_asset_ids() {
    const ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";
    let issuer_id = ids::account_id(ISSUER);
    // Native leg: type 0, empty code, issuer_id 0.
    assert_eq!(ids::pool_leg_asset_id(0, "", 0), ids::NATIVE_ASSET_ID);
    // credit_alphanum4 (XDR type 1) and credit_alphanum12 (XDR type 2) are
    // both classic credit, so both must land on the surrogate the writer
    // computes from the StrKey the claim atom carries.
    for (asset_type, code) in [(1i16, "TF"), (2i16, "CETES")] {
        assert_eq!(
            ids::pool_leg_asset_id(asset_type, code, issuer_id),
            ids::credit_asset_id(code, ISSUER),
            "leg {code} (XDR type {asset_type}) must match the written asset_id",
        );
    }
    // The bug this replaced: a type-2 leg resolved to 0, and 0 is an id no
    // row is ever stored under, so the leg could never match.
    assert_ne!(ids::pool_leg_asset_id(2, "CETES", issuer_id), 0);
}

/// The same equality against REAL production values, so the pin does not
/// rest on this module's own arithmetic being self-consistent.
///
/// Pool `8CA53441…` (yXLM / CETES) is the one that exposed task 0489: a
/// `credit_alphanum4` leg beside a `credit_alphanum12` one, so the page
/// rendered the first and dropped the second. Left column read from
/// `liquidity_pools`, right column the `DISTINCT asset_id` that
/// `lp_operation_amounts` actually holds for that pool — both captured
/// from prod on 2026-08-17. Static values, no network.
#[test]
fn pool_leg_surrogates_match_production_rows() {
    // (asset_type, code, issuer_id) -> the asset_id stored on prod
    for (asset_type, code, issuer_id, stored) in [
        (
            1i16,
            "yXLM",
            -5_950_609_493_839_131_376i64,
            258_332_573_254_456_524i64,
        ),
        (
            2i16,
            "CETES",
            1_238_723_897_090_515_379i64,
            4_032_595_941_348_833_451i64,
        ),
    ] {
        assert_eq!(
            ids::pool_leg_asset_id(asset_type, code, issuer_id),
            stored,
            "leg {code} must resolve to the asset_id production stores",
        );
    }
}

/// The SAC joins on both pool reads must not filter a leg out for having an
/// empty `asset_code` (task 0470).
///
/// An empty code is native XLM's real, stored identity — not a missing
/// value — and native has a deployed SAC. An `asset_code != ''` guard was
/// added deliberately in `a19ac8f6` to match Postgres, which returned NULL
/// there; Postgres is retired and `/v1/assets/native` publishes that same
/// SAC, so the guard left one asset describing itself two ways depending on
/// the endpoint.
///
/// Pinned on the module source because both queries are inline string
/// literals — there is no builder to call. That is the honest limit of this
/// guard: it catches the exact regression (a re-added `!= ''` on a leg
/// code) and nothing subtler. A behavioural test needs the queries
/// extracted first, which is recorded as an acceptance criterion on 0470.
#[test]
fn no_leg_code_guard_can_exclude_the_native_leg_from_its_sac() {
    // Only the production half — the test module below quotes the guard it
    // is looking for, and would match itself.
    let src = include_str!("queries.rs");
    let production = src.split("#[cfg(test)]").next().unwrap_or(src);
    // Count only the leg-code guards; other `!= ''` comparisons in this
    // module are about different columns and are none of this test's
    // business.
    let guards = production
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter(|l| l.contains("asset_a_code != ''") || l.contains("asset_b_code != ''"))
        .count();
    assert_eq!(
        guards, 0,
        "a leg-code guard is back: it silently drops native XLM's SAC, \
         which /v1/assets/native still reports"
    );
}

#[test]
fn fee_percent_formats() {
    assert_eq!(fee_percent_str(30), "0.3");
    assert_eq!(fee_percent_str(25), "0.25");
    assert_eq!(fee_percent_str(100), "1");
    assert_eq!(fee_percent_str(0), "0");
    assert_eq!(fee_percent_str(5), "0.05");
}

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

#[test]
fn asset_type_names() {
    assert_eq!(asset_type_name(0).as_deref(), Some("native"));
    assert_eq!(asset_type_name(1).as_deref(), Some("credit_alphanum4"));
    assert_eq!(asset_type_name(2).as_deref(), Some("credit_alphanum12"));
    assert_eq!(asset_type_name(3).as_deref(), Some("pool_share"));
    assert_eq!(asset_type_name(9), None);
}

/// The prices JOIN key contract (views.sql, pinned 2026-06-16):
/// native = ('native','XLM',''), classic = ('credit', code, issuer).
/// A wrong mapping here silently prices legs off the wrong row — the
/// exact failure mode the raw-`prices.assets` join produced (task 0199
/// activation note, bogus 96.4% coverage).
#[test]
fn price_leg_mapping() {
    let native = price_leg(0, None, None);
    assert_eq!(
        (native.kind, native.code.as_str(), native.issuer.as_str()),
        ("native", "XLM", "")
    );
    // Native ignores whatever code/issuer the row carries ('' / surrogate-0 artifacts).
    let native2 = price_leg(0, Some(""), Some(""));
    assert_eq!(native2.kind, "native");

    let usdc = price_leg(
        1,
        Some("USDC"),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
    );
    assert_eq!(usdc.kind, "credit");
    assert_eq!(usdc.code, "USDC");
    assert!(usdc.issuer.starts_with('G'));

    let alphanum12 = price_leg(2, Some("WGUARDIAN"), Some("GABC"));
    assert_eq!(alphanum12.kind, "credit");

    // Unpriceable degradations: missing identity parts or unexpected type
    // must match NO prices row (empty kind), never guess.
    assert_eq!(price_leg(1, None, Some("GABC")).kind, "");
    assert_eq!(price_leg(1, Some("USDC"), None).kind, "");
    assert_eq!(price_leg(1, Some(""), Some("GABC")).kind, "");
    assert_eq!(price_leg(3, Some("X"), Some("G")).kind, "");
    assert_eq!(price_leg(9, None, None).kind, "");
}

#[test]
fn usd_helpers() {
    assert_eq!(parse_f64("123.4567890"), Some(123.456789));
    assert_eq!(parse_f64("0"), Some(0.0));
    assert_eq!(parse_f64(""), None);
    assert_eq!(parse_f64("abc"), None);
    assert_eq!(parse_f64("inf"), None, "non-finite rejected");
    assert_eq!(usd_str(1234.5678), "1234.57");
    assert_eq!(usd_str(0.0), "0.00");
    // Sub-cent values must not collapse to "0.00" — a client cannot
    // tell that apart from a genuine zero (fee_revenue lives here).
    assert_eq!(usd_str(0.003), "0.0030");
    assert_eq!(usd_str(0.00009), "0.000090");
    assert_eq!(usd_str(-0.003), "-0.0030");
    // At or above a cent the plain money form still applies.
    assert_eq!(usd_str(0.01), "0.01");
    assert_eq!(usd_str(0.5), "0.50");
    // Fixed 2 decimals on every path — CH's toString(round(x, 2)) would
    // emit "25" / "1.5" / "0" here and split the wire shape between the
    // chart and the detail endpoint.
    assert_eq!(usd_str(25.0), "25.00");
    assert_eq!(usd_str(1.5), "1.50");
}

/// `fee_bps` is basis points: 30 bps = 0.30%, so the divisor is 10 000.
/// A /100 or /1000 slip inflates reported LP earnings 100× / 10×.
#[test]
fn fee_revenue_math() {
    assert_eq!(fee_revenue_usd(1_000_000.0, 30), 3_000.0);
    assert_eq!(fee_revenue_usd(1_000.0, 100), 10.0);
    assert_eq!(fee_revenue_usd(0.0, 30), 0.0);
    assert_eq!(fee_revenue_usd(500.0, 0), 0.0);
}
