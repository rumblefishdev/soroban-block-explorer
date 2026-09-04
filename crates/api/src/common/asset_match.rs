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
//! 2. **`native` is a SYNONYM, not a replacement.** Someone typing `native`
//!    means XLM — but 68 mainnet assets carry a code containing `NATIVE`, and
//!    they must not vanish because of a word swap. So the synonym is matched
//!    ALONGSIDE what was typed, and only the ranking prefers XLM.
//!
//! Everything here is a pure string builder, so every surface's SQL says the
//! same thing by construction rather than by review. Ranking is only used
//! where there is no cursor to resume — see the note in the `/v1/assets` seek
//! for why a ranked keyset costs far more than a ranked `LIMIT`.

/// The code a needle is a SYNONYM for, if it is one.
///
/// Only `native` today: it is what the native asset is called, `XLM` is what
/// it displays as. Returning it instead of replacing the needle is deliberate
/// — an earlier version swapped the word before the query was built, and the
/// 68 assets whose code contains `NATIVE` disappeared from a search for
/// `native` while still turning up for `nativ`. A shorter needle must never
/// find more than a longer one.
pub fn alias(q: &str) -> Option<&'static str> {
    q.trim().eq_ignore_ascii_case("native").then_some("XLM")
}

/// The needle used for RANKING: the synonym when there is one, else the needle
/// itself. Only the order is affected — matching still sees both.
pub fn rank_needle(q: &str) -> &str {
    alias(q).unwrap_or(q)
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
/// displayed code. **One bind**, or **two** when the needle has an [`alias`]:
/// the needle first, then the synonym.
pub fn matches_sql(shown: &str, with_alias: bool) -> String {
    let one = format!("position({shown}, lower(?)) > 0");
    if with_alias {
        format!("({one} OR position({shown}, lower(?)) > 0)")
    } else {
        one
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_is_a_synonym_and_never_hides_the_literal_code() {
        // `native` means XLM — but it must not swallow the assets whose code
        // really contains NATIVE. Both needles go into the predicate, so a
        // shorter query can never find more than a longer one.
        assert_eq!(alias("native"), Some("XLM"));
        assert_eq!(alias("NaTiVe"), Some("XLM"));
        assert_eq!(alias("nativ"), None);
        assert_eq!(alias("usdc"), None);

        let shown = shown_code("a.asset_type", "a.asset_code");
        let with = matches_sql(&shown, alias("native").is_some());
        assert_eq!(with.matches('?').count(), 2, "needle AND synonym: {with}");
        assert!(with.contains(" OR "), "{with}");
        let without = matches_sql(&shown, alias("usdc").is_some());
        assert_eq!(without.matches('?').count(), 1, "{without}");

        // Only the ORDER is steered by the synonym, so native XLM comes first
        // and the NATIVE-coded assets follow it instead of vanishing.
        assert_eq!(rank_needle("native"), "XLM");
        assert_eq!(rank_needle("usdc"), "usdc");
    }

    #[test]
    fn the_tier_reads_the_displayed_code_not_the_stored_one() {
        // Native stores an EMPTY code and renders as XLM. Comparing the stored
        // value is the bug that made `XLM` miss the one asset everybody meant.
        let shown = shown_code("a.asset_type", "a.asset_code");
        assert!(shown.contains("if(a.asset_type = 0, 'XLM'"), "{shown}");
        // Bind counts are part of the contract: callers match them 1:1.
        assert_eq!(tier_sql(&shown).matches('?').count(), 2);
        assert!(
            tier_sql(&shown).contains("startsWith"),
            "prefix arm required"
        );
    }
}
