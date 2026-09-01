//! Matching liquidity pools by asset code — the ONE definition, shared by the
//! pools list (`/v1/liquidity-pools`) and global search (`/v1/search`).
//!
//! It lives here because the two endpoints answered the same question
//! differently: task 0440 taught the pools list substring + `A/B` pair
//! matching, and global search kept matching pools on an exact `pool_id`
//! only, so `KALE` returned 58 pools on one surface and 0 on the other
//! (task 0470). A second copy of this rule would drift the same way — and
//! the native case below is precisely where a re-implementation goes wrong.

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

/// One leg's match test.
///
/// **Native XLM is stored with an EMPTY code**, so a bare
/// `positionCaseInsensitive(asset_a_code, 'XLM')` matches thousands of
/// impostor codes (`XLMFISH`, `yXLM`, …) and misses every real XLM pool. The
/// `if(type = 0, 'XLM', code)` arm is what makes the native case work; do not
/// simplify it away.
///
/// Both callers alias the pool row as `lp`, so the qualifier is fixed rather
/// than threaded through as a parameter.
fn leg(side: char) -> String {
    format!(
        "positionCaseInsensitive(if(lp.asset_{side}_type = 0, 'XLM', \
         lp.asset_{side}_code), ?) > 0"
    )
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
///
/// **Classic pools only** (`pool_kind = 0`). The pair columns this predicate
/// reads are defaults on soroban rows — `asset_a_type = 0` there is a
/// placeholder, not native, so without the guard the `if(type = 0, 'XLM', …)`
/// arm reads EVERY soroban pool as `XLM/XLM` (497 false positives measured on
/// prod, review #438 F2). Soroban legs are surrogate ids; matching them by
/// code is a join to the asset dimensions and lands with the legs migration,
/// not here.
pub fn asset_codes_predicate(codes: &[String]) -> Option<(String, Vec<String>)> {
    match codes {
        [one] => Some((
            format!("(lp.pool_kind = 0 AND ({} OR {}))", leg('a'), leg('b')),
            vec![one.clone(), one.clone()],
        )),
        [first, second] => Some((
            format!(
                "(lp.pool_kind = 0 AND (({a} AND {b}) OR ({a} AND {b})))",
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
    }

    #[test]
    fn predicate_is_gated_to_classic_pools() {
        // Soroban registry rows carry the pair columns at defaults, so
        // `asset_a_type = 0` there is a placeholder, not native — without
        // this gate every soroban pool matches `XLM/XLM` (review #438 F2).
        for raw in ["xlm", "xlm/usdc"] {
            let (sql, _) = asset_codes_predicate(&codes(raw)).expect("clause");
            assert!(sql.starts_with("(lp.pool_kind = 0 AND "), "{sql}");
        }
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
        let (sql, _) = asset_codes_predicate(&codes("xlm")).expect("clause");
        assert!(sql.contains("if(lp.asset_a_type = 0, 'XLM', lp.asset_a_code)"));
        assert!(sql.contains("if(lp.asset_b_type = 0, 'XLM', lp.asset_b_code)"));
    }
}
