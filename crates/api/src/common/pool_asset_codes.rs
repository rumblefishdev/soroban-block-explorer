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
/// trim+uppercase normalization matches caller intent for a free-text field.
/// No endpoint offers exact, issuer-disambiguated matching: a caller who needs
/// one asset exactly pastes its pool identifier instead.
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

/// The set of `assets.id` whose DISPLAYED code contains the needle.
///
/// The display rule itself comes from `common::asset_identity` — the assets
/// list and global search rank on the same expression, and this used to be a
/// third hand-written copy of it.
///
/// The set is matched against `legs`, which both pool kinds fill with asset
/// surrogates, so one rule answers for both. The pair columns it used to read
/// are classic-only, which is why the same filter used to return every soroban
/// pool as a false `XLM` hit: their placeholder leg types read as native.
fn matching_assets() -> String {
    format!(
        "SELECT id FROM assets WHERE position({shown}, lower(?)) > 0",
        shown = crate::common::asset_identity::shown_code_sql(""),
    )
}

/// Boolean expression matching pools against `codes`, plus its bind values in
/// left-to-right `?` order. `None` when there is nothing to match on — the
/// caller then adds no clause at all.
///
/// A pair assigns each needle its OWN leg, rather than asking each needle
/// independently whether it matches somewhere. The difference only shows when
/// the needles overlap, and then it is the whole answer: `USDC/USDC` means the
/// pools with USDC on both sides, not every pool with USDC anywhere. Same for a
/// needle that is a prefix of the other (`USD/USDC`) — one leg must not satisfy
/// both halves.
///
/// Over a pair that was two columns; over a list it is Hall's condition for two
/// sets, which for legs reads: something matches the first needle, something
/// matches the second, and at least TWO legs match either. Without the last
/// clause a single USDC leg would satisfy `USDC/USDC` on its own.
pub fn asset_codes_predicate(codes: &[String]) -> Option<(String, Vec<String>)> {
    let set = matching_assets();
    // The two halves of a pair carry the SAME expression — only the bind
    // distinguishes them, which is why this is one string and not an `a` and
    // a `b` that a reader has to diff before finding they are identical.
    let has_leg = format!("arrayExists(x -> x IN ({set}), lp.legs)");
    match codes {
        [one] => Some((has_leg, vec![one.clone()])),
        [first, second] => Some((
            format!(
                "({has_leg} AND {has_leg} \
                  AND arrayCount(x -> x IN ({set}) OR x IN ({set}), lp.legs) >= 2)"
            ),
            // Bind order follows the `?`s left to right: the two `arrayExists`
            // sets, then the two inside the count.
            vec![first.clone(), second.clone(), first.clone(), second.clone()],
        )),
        // `normalize_asset_codes` yields at most two needles; zero means no
        // filter was asked for.
        _ => None,
    }
}

#[cfg(test)]
mod tests;
