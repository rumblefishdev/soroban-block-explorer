//! Matching liquidity pools by asset code — the ONE definition, shared by the
//! pools list (`/v1/liquidity-pools`) and global search (`/v1/search`).
//!
//! It lives here because the two endpoints answered the same question
//! differently: task 0440 taught the pools list substring + `A/B` pair
//! matching, and global search kept matching pools on an exact `pool_id`
//! only, so `KALE` returned 58 pools on one surface and 0 on the other
//! (task 0470). A second copy of this rule would drift the same way — and
//! the native case below is precisely where a re-implementation goes wrong.

use crate::common::asset_match;

/// Split a free-text asset filter into at most two needles.
///
/// Stellar protocol asset codes are case-sensitive (1–12 ASCII chars, any
/// case), but the canonical convention is uppercase (USDC, XLM). The
/// trim+uppercase normalization matches caller intent for a free-text field;
/// consumers who need exact case-sensitive issuer-disambiguated matching
/// should use a per-leg `(code, issuer)` mode instead.
///
/// `splitn(2, '/')` caps the result at two: a third slash stays inside the
/// second needle rather than silently becoming an extra constraint.
pub fn normalize_asset_codes(raw: Option<String>) -> Vec<String> {
    raw.map(|s| s.trim().to_uppercase())
        .into_iter()
        .flat_map(|s| {
            s.splitn(2, '/')
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// One leg's displayed code — what the pool row RENDERS as, which for a
/// native leg is `XLM` and not the empty string it stores.
///
/// Both matching and ranking go through this, and both come from
/// `common::asset_match`, so a pool leg and an asset row answer "does this
/// match" and "how well" with the same rule. Before that they were separate
/// spellings that agreed by accident.
fn leg_shown(side: char, alias: &str) -> String {
    asset_match::shown_code(
        &format!("{alias}.asset_{side}_type"),
        &format!("{alias}.asset_{side}_code"),
    )
}

/// One leg's match test. One bind, the needle. Pool codes carry no synonym
/// arm: a pair query already names its legs, and `normalize_asset_codes`
/// upper-cases the needles before they get here.
fn leg(side: char) -> String {
    asset_match::matches_sql(&leg_shown(side, "lp"), false)
}

/// One leg's match TIER. Two binds, both the needle.
fn leg_tier(side: char, alias: &str) -> String {
    asset_match::tier_sql(&leg_shown(side, alias))
}

/// How well a pool matches `codes` — the pools' answer to the same question
/// the assets list answers with `match_tier`, so `XLM` puts real XLM pools
/// above the `yXLM` / `XLMFISH` look-alikes instead of trusting that the real
/// ones happen to be the busiest.
///
/// **Negated**, because the pool list's whole keyset runs DESC and a
/// mixed-direction keyset is not one comparison: `-0` beats `-1` beats `-2`
/// under DESC, so the best shelf still comes first.
///
/// One needle takes the pool's BEST leg (`least`) — the needle only has to be
/// satisfied once. A pair takes the WORSE leg of each assignment (`greatest`)
/// and then the better assignment (`least`): both needles must be satisfied,
/// and the pool is only as good as its weaker half. That mirrors how
/// [`asset_codes_predicate`] already assigns each needle its own leg.
///
/// `None` when there is nothing to rank by — the caller then orders as before.
pub fn asset_codes_rank(codes: &[String], alias: &str) -> Option<(String, Vec<String>)> {
    let a = leg_tier('a', alias);
    let b = leg_tier('b', alias);
    match codes {
        [one] => Some((format!("-toInt16(least({a}, {b}))"), vec![one.clone(); 4])),
        [first, second] => Some((
            format!("-toInt16(least(greatest({a}, {b}), greatest({a}, {b})))"),
            // Bind order follows the `?`s left to right: the first assignment
            // (first -> a, second -> b), then the reversed one.
            vec![
                first.clone(),
                first.clone(),
                second.clone(),
                second.clone(),
                second.clone(),
                second.clone(),
                first.clone(),
                first.clone(),
            ],
        )),
        _ => None,
    }
}

/// Boolean expression matching pools against `codes`, plus its bind values in
/// left-to-right `?` order. `None` when there is nothing to match on — the
/// caller then adds no clause at all.
///
/// A pair assigns each needle its OWN leg, in either order, rather than asking
/// each needle independently whether it matches somewhere. The difference only
/// shows when the needles overlap, and then it is the whole answer:
/// `USDC/USDC` means the 72 pools with USDC on both sides, not the 2 912 with
/// USDC anywhere. Same for a needle that is a prefix of the other
/// (`USD/USDC`) — one asset must not satisfy both halves of the query.
pub fn asset_codes_predicate(codes: &[String]) -> Option<(String, Vec<String>)> {
    match codes {
        [one] => Some((
            format!("({} OR {})", leg('a'), leg('b')),
            vec![one.clone(), one.clone()],
        )),
        [first, second] => Some((
            format!(
                "(({a} AND {b}) OR ({a} AND {b}))",
                a = leg('a'),
                b = leg('b'),
            ),
            // Bind order follows the `?`s left to right: first/second, then
            // the reversed assignment.
            vec![first.clone(), second.clone(), second.clone(), first.clone()],
        )),
        // `normalize_asset_codes` yields at most two needles; zero means no
        // filter was asked for.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
        // Asserted through the shared builder rather than against a literal:
        // the rule lives in `common::asset_match` now (task 0485), and a test
        // pinning one spelling of it is exactly what let four spellings drift
        // apart in the first place.
        let (sql, _) = asset_codes_predicate(&codes("xlm")).expect("clause");
        for side in ['a', 'b'] {
            let shown = asset_match::shown_code(
                &format!("lp.asset_{side}_type"),
                &format!("lp.asset_{side}_code"),
            );
            assert!(
                sql.contains(&shown),
                "leg {side} lost the native alias: {sql}"
            );
        }
    }
}
