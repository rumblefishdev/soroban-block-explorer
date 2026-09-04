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
//! 2. **No synonyms.** An earlier version treated `native` as a way to type
//!    `XLM` and swapped the word before building the query — which made the 68
//!    mainnet assets whose code contains `NATIVE` disappear from a search for
//!    `native` while still turning up for `nativ`. Matching both was tried and
//!    then dropped as unearned complexity: a needle matches what it literally
//!    says, and `XLM` is what native displays as, so `xlm` finds it.
//!
//! Everything here is a pure string builder, so every surface's SQL says the
//! same thing by construction rather than by review. Ranking is only used
//! where there is no cursor to resume — see the note in the `/v1/assets` seek
//! for why a ranked keyset costs far more than a ranked `LIMIT`.

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
/// displayed code. **One bind**, the needle.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_reads_the_displayed_code_not_the_stored_one() {
        // Native stores an EMPTY code and renders as XLM. Comparing the stored
        // value is the bug that made `XLM` miss the one asset everybody meant,
        // and it is also what makes `xlm` reach native without a special arm.
        let shown = shown_code("a.asset_type", "a.asset_code");
        assert!(shown.contains("if(a.asset_type = 0, 'XLM'"), "{shown}");
        assert!(
            tier_sql(&shown).contains("startsWith"),
            "prefix arm required"
        );
        // Bind counts are part of the contract: callers match them 1:1.
        assert_eq!(matches_sql(&shown).matches('?').count(), 1);
        assert_eq!(tier_sql(&shown).matches('?').count(), 2);
    }
}
