use super::*;
// Still needed HERE and nowhere else: the read path stopped computing
// leg surrogates when it started reading the stored ones, but the test
// below still pins that the writer's formula is the one the amount
// rows are keyed by.
use db_clickhouse::persist::ids;

/// The leg surrogates the indexer stores in `liquidity_pools.legs` MUST
/// equal the ones it writes into `lp_operation_amounts.asset_id` from a
/// claim atom's asset string (`stage.rs::claim_atom_asset_id` →
/// `ids::credit_asset_id` / `NATIVE_ASSET_ID`). They meet only through this
/// equality: if it breaks, no row ever matches a leg and the Amount column
/// silently goes blank instead of failing.
///
/// This is now the ONLY reason the read path still knows the writer's
/// formula. It stopped COMPUTING leg surrogates when it started reading the
/// stored ones (task 0374), so the equality has no other witness — which is
/// exactly why the test stays here, pinning the two producers against each
/// other rather than against a constant.
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

#[test]
fn fee_percent_formats() {
    assert_eq!(fee_percent_str(30), "0.3");
    assert_eq!(fee_percent_str(25), "0.25");
    assert_eq!(fee_percent_str(100), "1");
    assert_eq!(fee_percent_str(0), "0");
    assert_eq!(fee_percent_str(5), "0.05");
}

/// The leg's kind label comes from the domain enum, not from a local match
/// — the local one spoke the XDR vocabulary while the sibling endpoint
/// spoke the family one, and only `native` coincided.
#[test]
fn a_leg_is_named_in_the_family_vocabulary() {
    let name = |t: i16| {
        domain::AssetFamily::try_from(t)
            .ok()
            .map(|f| f.as_str().to_string())
    };
    assert_eq!(name(0).as_deref(), Some("native"));
    assert_eq!(name(1).as_deref(), Some("classic_credit"));
    assert_eq!(name(3).as_deref(), Some("soroban"));
    // 2 is the retired SAC facet (ADR 0051) and 9 is nothing at all.
    assert_eq!(name(2), None);
    assert_eq!(name(9), None);
}

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
