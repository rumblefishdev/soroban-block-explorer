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
    assert!(asset_codes_predicate(&[]).is_none());
}

#[test]
fn single_needle_tests_both_legs_and_binds_twice() {
    let (sql, binds) = asset_codes_predicate(&codes("kale")).expect("clause");
    assert_eq!(binds, vec!["KALE", "KALE"]);
    assert_eq!(sql.matches('?').count(), 2);
    assert!(sql.contains(" OR "));
    assert!(!sql.contains(" AND "));
}

#[test]
fn pair_binds_both_assignments_so_order_does_not_matter() {
    let (sql, binds) = asset_codes_predicate(&codes("xlm/kale")).expect("clause");
    assert_eq!(binds, vec!["XLM", "KALE", "KALE", "XLM"]);
    assert_eq!(sql.matches('?').count(), 4);
}

#[test]
fn native_leg_is_matched_by_type_not_by_code() {
    // Load-bearing: without the `type = 0` arm, `XLM` matches impostor
    // codes and misses every real XLM pool (task 0440).
    //
    let (sql, _) = asset_codes_predicate(&codes("xlm")).expect("clause");
    for side in ['a', 'b'] {
        let shown = leg_shown(side, "lp");
        assert!(
            sql.contains(&shown),
            "leg {side} lost the native alias: {sql}"
        );
    }
}
