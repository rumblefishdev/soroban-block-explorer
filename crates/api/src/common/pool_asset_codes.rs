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

/// Boolean expression matching pools against `codes`, plus its bind values in
/// left-to-right `?` order. `None` when there is nothing to match on — the
/// caller then adds no clause at all.
///
/// One code: some leg's asset displays a code containing it (native as `XLM`).
///
/// Two codes: each on its OWN leg — both match somewhere, and at least two
/// different legs match between them. Without the last clause one USDC leg
/// would satisfy `USDC/USDC` (or `USD/USDC`) on its own.
pub fn asset_codes_predicate(codes: &[String]) -> Option<(String, Vec<String>)> {
    let leg_matches = format!(
        "x IN (SELECT id FROM assets WHERE position({}, lower(?)) > 0)",
        crate::common::asset_identity::shown_code_sql(""),
    );
    let m = &leg_matches;
    match codes {
        [one] => Some((format!("arrayExists(x -> {m}, lp.legs)"), vec![one.clone()])),
        [first, second] => Some((
            format!(
                "(arrayExists(x -> {m}, lp.legs) AND arrayExists(x -> {m}, lp.legs) \
                  AND arrayCount(x -> {m} OR {m}, lp.legs) >= 2)"
            ),
            // One bind per `?`, left to right.
            vec![first.clone(), second.clone(), first.clone(), second.clone()],
        )),
        // `normalize_asset_codes` yields at most two needles; zero means no
        // filter was asked for.
        _ => None,
    }
}

#[cfg(test)]
mod tests;
