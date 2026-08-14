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

/// The same free-text box also has to accept a pool's own identifier.
///
/// Without this, pasting `LD7K…` into the pools filter runs it through
/// `normalize_asset_codes` and matches it as a substring of an asset CODE —
/// which finds nothing, so the page answers "no such pool" about a pool that
/// exists. A confident wrong answer, not a missing convenience (task 0470).
///
/// **StrKey only.** Task 0264 made the `L…` SEP-23 form canonical across every
/// surface and dropped hex deliberately — the project was pre-deploy, so there
/// were no hex bookmarks to preserve — and `path::pool_id_strkey` still tells
/// callers that "hex form is no longer accepted".
///
/// `search::classifier` does accept 64-char hex, but that is the one endpoint
/// 0264 explicitly deferred, not a precedent. Copying it here would spread a
/// known exception into a third place.
///
/// Dropping hex also removes a trap: `hex::decode` accepts any even-length
/// string over `[0-9a-f]`, so `FACE`, `BEEF` and `CAFE` — all valid asset
/// codes — would parse as identifiers and search for a pool that does not
/// exist.
///
/// Returns the lowercase hex the `pool_id` column stores.
pub fn pool_id_from_text(raw: &str) -> Option<String> {
    match stellar_strkey::LiquidityPool::from_string(raw.trim()) {
        Ok(stellar_strkey::LiquidityPool(bytes)) => Some(hex::encode(bytes)),
        Err(_) => None,
    }
}

/// One leg's match test.
///
/// **Native XLM is stored with an EMPTY code**, so a bare
/// `positionCaseInsensitive(asset_a_code, 'XLM')` matches thousands of
/// impostor codes (`XLMFISH`, `yXLM`, …) and misses every real XLM pool. The
/// `if(type = 0, 'XLM', code)` arm is what makes the native case work; do not
/// simplify it away.
///
/// `qualifier` prefixes the column (`"lp."` inside the list's join, `""` over
/// a bare subquery); the column names themselves are fixed.
fn leg(qualifier: &str, side: char) -> String {
    format!(
        "positionCaseInsensitive(if({qualifier}asset_{side}_type = 0, 'XLM', \
         {qualifier}asset_{side}_code), ?) > 0"
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
pub fn asset_codes_predicate(qualifier: &str, codes: &[String]) -> Option<(String, Vec<String>)> {
    match codes {
        [one] => Some((
            format!("({} OR {})", leg(qualifier, 'a'), leg(qualifier, 'b')),
            vec![one.clone(), one.clone()],
        )),
        [first, second] => Some((
            format!(
                "(({a} AND {b}) OR ({a} AND {b}))",
                a = leg(qualifier, 'a'),
                b = leg(qualifier, 'b'),
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
        assert!(asset_codes_predicate("lp.", &[]).is_none());
    }

    #[test]
    fn single_needle_tests_both_legs_and_binds_twice() {
        let (sql, binds) = asset_codes_predicate("lp.", &codes("kale")).expect("clause");
        assert_eq!(binds, vec!["KALE", "KALE"]);
        assert_eq!(sql.matches('?').count(), 2);
        assert!(sql.contains(" OR "));
        assert!(!sql.contains(" AND "));
    }

    #[test]
    fn pair_binds_both_assignments_so_order_does_not_matter() {
        let (sql, binds) = asset_codes_predicate("lp.", &codes("xlm/kale")).expect("clause");
        assert_eq!(binds, vec!["XLM", "KALE", "KALE", "XLM"]);
        assert_eq!(sql.matches('?').count(), 4);
    }

    #[test]
    fn native_leg_is_matched_by_type_not_by_code() {
        // Load-bearing: without the `type = 0` arm, `XLM` matches impostor
        // codes and misses every real XLM pool (task 0440).
        let (sql, _) = asset_codes_predicate("lp.", &codes("xlm")).expect("clause");
        assert!(sql.contains("if(lp.asset_a_type = 0, 'XLM', lp.asset_a_code)"));
        assert!(sql.contains("if(lp.asset_b_type = 0, 'XLM', lp.asset_b_code)"));
    }

    #[test]
    fn pool_identifier_is_recognised_as_a_strkey() {
        // Round-trip rather than a hand-typed constant: the invariant is that
        // the StrKey resolves to the hex of the SAME 32 bytes. A literal typed
        // by hand would only test whether the literal was right.
        let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        let hex = hex::encode(bytes);
        let strkey = stellar_strkey::LiquidityPool(bytes).to_string();
        assert!(
            strkey.starts_with('L'),
            "expected an L-strkey, got {strkey}"
        );

        assert_eq!(pool_id_from_text(&strkey).as_deref(), Some(hex.as_str()));
        // Pasting from a terminal or a chat window brings whitespace along.
        assert_eq!(
            pool_id_from_text(&format!("  {strkey} ")).as_deref(),
            Some(hex.as_str())
        );
        // Hex is NOT an identifier here — 0264 made StrKey the only accepted
        // form, and `path::pool_id_strkey` rejects hex on the detail route.
        assert!(pool_id_from_text(&hex).is_none());
        assert!(pool_id_from_text(&hex.to_uppercase()).is_none());
    }

    #[test]
    fn an_asset_code_is_not_mistaken_for_an_identifier() {
        // The whole point of the split: these must fall through to the code
        // matcher, not become a point seek that finds nothing.
        assert!(pool_id_from_text("XLM").is_none());
        assert!(pool_id_from_text("xlm/kale").is_none());
        assert!(pool_id_from_text("").is_none());
        // 64 chars but not a StrKey.
        assert!(pool_id_from_text(&"z".repeat(64)).is_none());
        // Hex-looking asset codes stay asset codes. With a hex branch these
        // parsed as identifiers and searched for a pool that does not exist.
        for code in ["FACE", "BEEF", "CAFE", "DEAD"] {
            assert!(
                pool_id_from_text(code).is_none(),
                "{code} must stay an asset code"
            );
        }
        // A strkey of the wrong type — accounts are not pools.
        assert!(
            pool_id_from_text("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN").is_none()
        );
    }

    #[test]
    fn qualifier_is_applied_to_every_column() {
        let (bare, _) = asset_codes_predicate("", &codes("kale")).expect("clause");
        assert!(bare.contains("if(asset_a_type = 0, 'XLM', asset_a_code)"));
        assert!(!bare.contains("lp."));
    }
}
