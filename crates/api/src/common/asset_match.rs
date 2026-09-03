//! One definition of "how well does this asset match what the user typed",
//! shared by every surface that answers that question (task 0485).
//!
//! It was four definitions before this module: the `/v1/search` asset bucket
//! compared the RAW `asset_code` and bolted a native special-case onto the
//! side, the `/v1/assets` list compared the DISPLAYED code with a different
//! native arm, the pools predicate had a third spelling, and the Rust cursor a
//! fourth. All four agreed by accident, not by construction.
//!
//! Two ideas carry the whole thing:
//!
//! 1. **Compare what the row DISPLAYS as, never what it stores.** Native XLM
//!    is stored as `asset_type = 0` with an EMPTY code while every surface
//!    renders it `XLM`. Comparing the stored value is why `XLM` used to return
//!    thousands of impostor codes and miss the one asset everybody meant.
//! 2. **Normalise the needle once, at the door.** `native` is just another way
//!    to type `XLM`. Fold it there and no SQL downstream needs a special arm
//!    for it — which is what let three of the four spellings differ.
//!
//! Everything here is a pure string builder or a pure function, so the SQL and
//! its Rust twin sit in one file and can be read side by side. That twin is
//! load-bearing: a keyset cursor has to record the tier of the row it stopped
//! on, and the handler only ever sees the finished row, never the SQL.

/// The needle as every surface should treat it: trimmed, and with `native`
/// folded onto the code native actually displays as.
///
/// Callers normalise ONCE, on the way in. A surface that skips this still
/// works for `xlm` and silently loses `native`.
pub fn normalize_needle(q: &str) -> String {
    let q = q.trim();
    if q.eq_ignore_ascii_case("native") {
        "XLM".to_string()
    } else {
        q.to_string()
    }
}

/// SQL for the code a row DISPLAYS as, lower-cased — native's `XLM` standing
/// in for its empty stored code.
///
/// Takes the two column expressions rather than a table alias so the pools
/// side (`lp.asset_a_type` / `lp.asset_a_code`) and the assets side
/// (`a.asset_type` / `a.asset_code`) can both use it.
pub fn shown_code(type_expr: &str, code_expr: &str) -> String {
    format!("lower(if({type_expr} = 0, 'XLM', toString({code_expr})))")
}

/// SQL for "does this row match at all" — a substring test against the
/// displayed code. **One bind**, the normalised needle.
pub fn matches_sql(shown: &str) -> String {
    format!("position({shown}, lower(?)) > 0")
}

/// SQL for the match TIER: `0` the needle IS the whole displayed code, `1` the
/// code starts with it, `2` it appears somewhere inside. **Two binds**, both
/// the normalised needle.
///
/// A tier, not a score. The order follows from what MATCHED, so there is no
/// invented weighting to defend and nothing to re-tune when a result looks
/// wrong. Ranking beyond this — which of two exact matches wins — is the
/// caller's tie-break, because the answer differs per surface (holder count
/// for assets, activity for pools).
pub fn tier_sql(shown: &str) -> String {
    format!("multiIf({shown} = lower(?), 0, startsWith({shown}, lower(?)), 1, 2)")
}

/// The Rust twin of [`tier_sql`], over the same DISPLAYED code.
///
/// Exists because a ranked keyset resumes on the tier of the page's last row,
/// and that cursor is built in Rust. Drift between the two is silent — it
/// skips rows at a page boundary — so the guard is a page-walk test that
/// demands N pages equal one page of N×size.
pub fn tier(needle: &str, shown_code: &str) -> u8 {
    let needle = needle.to_lowercase();
    let shown = shown_code.to_lowercase();
    if shown == needle {
        0
    } else if shown.starts_with(&needle) {
        1
    } else {
        2
    }
}

/// The displayed code of an asset row, for [`tier`] — the Rust twin of
/// [`shown_code`]. `None` / empty means "no code", which is native's stored
/// state and also a Soroban-native token's.
pub fn shown_code_of(asset_type: i16, asset_code: Option<&str>) -> &str {
    if asset_type == 0 {
        "XLM"
    } else {
        asset_code.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_reachable_by_both_spellings_without_a_special_arm() {
        // The whole point of normalising at the door: after this, `native`
        // and `xlm` are the same query, and no SQL below needs to know.
        assert_eq!(normalize_needle("native"), "XLM");
        assert_eq!(normalize_needle("NaTiVe"), "XLM");
        assert_eq!(normalize_needle(" usdc "), "usdc");
        // Native's stored code is empty; it is matched on what it displays as.
        assert_eq!(shown_code_of(0, None), "XLM");
        assert_eq!(
            tier(
                &normalize_needle("native").to_lowercase(),
                shown_code_of(0, None)
            ),
            0
        );
        assert_eq!(tier("xlm", shown_code_of(0, None)), 0);
    }

    #[test]
    fn tiers_are_exact_then_prefix_then_anywhere() {
        assert_eq!(tier("usdc", "USDC"), 0);
        assert_eq!(tier("USDC", "usdc"), 0);
        assert_eq!(tier("xlm", "XLMFISH"), 1);
        assert_eq!(tier("xlm", "yXLM"), 2);
        // Total: a row matched on its on-chain name rather than its code still
        // gets an answer.
        assert_eq!(tier("spiko", ""), 2);
    }

    #[test]
    fn the_sql_and_its_twin_read_the_same_column() {
        // Both sides must compare the DISPLAYED code — this is the bug that
        // made `XLM` miss native, and it comes back the moment one side is
        // rewritten against `asset_code` directly.
        let shown = shown_code("a.asset_type", "a.asset_code");
        assert!(shown.contains("if(a.asset_type = 0, 'XLM'"), "{shown}");
        assert!(
            tier_sql(&shown).contains("startsWith"),
            "tier must have a prefix arm"
        );
        // Bind counts are part of the contract: callers match them 1:1.
        assert_eq!(matches_sql(&shown).matches('?').count(), 1);
        assert_eq!(tier_sql(&shown).matches('?').count(), 2);
    }
}
