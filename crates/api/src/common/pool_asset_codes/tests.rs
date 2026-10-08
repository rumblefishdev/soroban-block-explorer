use super::*;

fn codes(raw: &str) -> Vec<String> {
    normalize_asset_codes(Some(raw.to_string()))
}

#[test]
fn uppercases_and_trims() {
    assert_eq!(codes("  usdc "), vec!["USDC"]);
}

#[test]
fn splits_a_pair_on_the_slash() {
    assert_eq!(codes("xlm/kale"), vec!["XLM", "KALE"]);
}

#[test]
fn third_code_stays_inside_the_second_needle() {
    // Not three constraints — the second needle keeps the rest verbatim,
    // so the query matches nothing rather than quietly dropping a code.
    assert_eq!(codes("a/b/c"), vec!["A", "B/C"]);
}

#[test]
fn empty_and_blank_yield_no_needles() {
    assert!(codes("").is_empty());
    assert!(codes("   ").is_empty());
    assert!(codes("/").is_empty());
    assert!(normalize_asset_codes(None).is_empty());
}

#[test]
fn no_needles_means_no_clause() {
    assert!(asset_codes_predicate(&[], &[]).is_none());
}

#[test]
fn a_single_needle_asks_the_legs_once() {
    let f = asset_codes_predicate(&codes("kale"), &[vec![]]).expect("clause");
    assert_eq!(f.binds, vec!["KALE"]);
    assert_eq!(f.sql.matches('?').count(), 1);
    assert!(f.sql.contains("arrayExists"), "{}", f.sql);
    assert!(
        !f.sql.contains("arrayCount"),
        "one needle needs no distinctness"
    );
}

/// Order does not matter because neither needle is tied to a position —
/// they are tied to DISTINCT legs, which the count enforces.
#[test]
fn a_pair_needs_two_distinct_legs() {
    let f = asset_codes_predicate(&codes("xlm/kale"), &[vec![], vec![]]).expect("clause");
    assert_eq!(f.binds, vec!["XLM", "KALE", "XLM", "KALE"]);
    assert_eq!(f.sql.matches('?').count(), 4);
    assert!(
        f.sql.contains(">= 2"),
        "the distinctness clause is missing: {}",
        f.sql
    );
}

/// The case the distinctness clause exists for: one leg cannot answer both
/// halves of `USDC/USDC`.
#[test]
fn a_repeated_needle_still_needs_two_legs() {
    let f = asset_codes_predicate(&codes("usdc/usdc"), &[vec![], vec![]]).expect("clause");
    assert_eq!(f.binds, vec!["USDC", "USDC", "USDC", "USDC"]);
    assert!(f.sql.contains("arrayCount"), "{}", f.sql);
}

/// Load-bearing: without the `type = 0` arm, `XLM` matches impostor codes
/// and misses every real XLM pool (task 0440).
#[test]
fn native_is_matched_by_type_not_by_code() {
    let sql = asset_codes_predicate(&codes("xlm"), &[vec![]])
        .expect("clause")
        .sql;
    assert!(
        sql.contains("if(asset_type = 0, 'XLM'"),
        "the native alias is gone: {sql}"
    );
}

/// The predicate reads `legs`, never the legacy pair columns — which is
/// what makes it answer for a soroban pool at all.
#[test]
fn it_reads_legs_and_not_the_pair_columns() {
    let sql = asset_codes_predicate(&codes("usdc"), &[vec![]])
        .expect("clause")
        .sql;
    assert!(sql.contains("lp.legs"), "{sql}");
    assert!(!sql.contains("asset_a_"), "{sql}");
    assert!(!sql.contains("asset_b_"), "{sql}");
}

/// A Soroban token stores no code: the ids `soroban_token_ids` found for a
/// needle join its leg match as a server parameter — and a needle matching too
/// many tokens (a single letter) reads them in the query instead (task 0636).
#[test]
fn a_needles_soroban_tokens_join_its_own_leg_match() {
    let f = asset_codes_predicate(&codes("xlm/solvbtc"), &[vec![], vec![-42, 7]]).expect("clause");
    assert!(f.sql.contains("x IN {tokens_1:Array(Int64)}"), "{}", f.sql);
    assert!(!f.sql.contains("tokens_0"), "{}", f.sql);
    assert_eq!(f.params, vec![("tokens_1".to_string(), vec![-42, 7])]);
    assert_eq!(f.binds.len(), 4);

    let broad = vec![1; MAX_TOKEN_IDS_AS_PARAM + 1];
    let f = asset_codes_predicate(&codes("s"), &[broad]).expect("clause");
    assert!(f.params.is_empty());
    assert!(f.sql.contains("soroban_contract_metadata"), "{}", f.sql);
    assert_eq!(f.binds, vec!["S"; 3]);
}
