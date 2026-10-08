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

/// The pool filter for `codes`, Soroban tokens included: reads each needle's
/// tokens ([`soroban_token_ids`]) and builds [`asset_codes_predicate`]. `None`
/// when there is nothing to match on. Shared by the pools list and search.
pub async fn pool_asset_filter(
    client: &clickhouse::Client,
    codes: &[String],
) -> Result<Option<PoolAssetFilter>, clickhouse::error::Error> {
    let mut token_ids = Vec::new();
    for code in codes {
        token_ids.push(soroban_token_ids(client, code).await?);
    }
    Ok(asset_codes_predicate(codes, &token_ids))
}

/// The Soroban tokens whose self-declared name or symbol contains `needle`,
/// as asset ids (a Soroban token's asset id is its contract surrogate). A
/// Soroban token stores no code, so its symbol and name are what a user types
/// (task 0636). One past [`MAX_TOKEN_IDS_AS_PARAM`] is enough to choose the
/// in-query read, so no more are fetched.
pub async fn soroban_token_ids(
    client: &clickhouse::Client,
    needle: &str,
) -> Result<Vec<i64>, clickhouse::error::Error> {
    let contracts: Vec<String> = client
        .query(&format!(
            "SELECT contract_id FROM soroban_contract_metadata FINAL \
             WHERE positionCaseInsensitive(coalesce(name, ''), ?) > 0 \
                OR positionCaseInsensitive(coalesce(symbol, ''), ?) > 0 \
             LIMIT {}",
            MAX_TOKEN_IDS_AS_PARAM + 1
        ))
        .bind(needle)
        .bind(needle)
        .fetch_all()
        .await?;
    Ok(contracts
        .iter()
        .map(|c| db_clickhouse::persist::ids::contract_id(c))
        .collect())
}

/// Most Soroban tokens one needle may pass as a server parameter. Parameters
/// travel in the request URI, which the HTTP client caps at 64 KiB (~23 bytes
/// per id); 1,000 ids per needle keeps two needles near 46 KB, under the cap.
/// Real symbols and names stay far below (`USD` matches 282 tokens, `XRP` 15);
/// a single letter matches 3,000–3,700 (measured 2026-10-08).
const MAX_TOKEN_IDS_AS_PARAM: usize = 1_000;

/// A pool filter: the boolean expression, its `?` binds in left-to-right
/// order, and the server parameters (`{tokens_i:Array(Int64)}`) it reads.
pub struct PoolAssetFilter {
    pub sql: String,
    pub binds: Vec<String>,
    pub params: Vec<(String, Vec<i64>)>,
}

/// The filter matching pools against `codes`; `None` when there is nothing to
/// match on — the caller then adds no clause at all. `token_ids[i]` are
/// [`soroban_token_ids`] of `codes[i]`.
///
/// One code: some leg's asset displays a code containing it (native as
/// `XLM`), or is one of its Soroban tokens.
///
/// Two codes: each on its OWN leg — both match somewhere, and at least two
/// different legs match between them. Without the last clause one USDC leg
/// would satisfy `USDC/USDC` (or `USD/USDC`) on its own.
///
/// The token ids go in as a server parameter — a constant set, hashed once.
/// Measured on production data 2026-10-08: read inside the query they cost
/// +2.5M rows and +0.25 s per pool-list call (the subquery is re-read per
/// block inside the lambda); inlined, two short needles exceed
/// `max_query_size`; as a `has()` array, 9–33 s. Only a needle matching more
/// than [`MAX_TOKEN_IDS_AS_PARAM`] tokens takes the in-query subquery.
pub fn asset_codes_predicate(codes: &[String], token_ids: &[Vec<i64>]) -> Option<PoolAssetFilter> {
    let mut params = Vec::new();
    let mut legs = Vec::new();
    for (i, code) in codes.iter().enumerate() {
        legs.push(leg_matches(i, code, &token_ids[i], &mut params));
    }
    match legs.as_slice() {
        [(m, b)] => Some(PoolAssetFilter {
            sql: format!("arrayExists(x -> {m}, lp.legs)"),
            binds: b.clone(),
            params,
        }),
        [(m0, b0), (m1, b1)] => Some(PoolAssetFilter {
            sql: format!(
                "(arrayExists(x -> {m0}, lp.legs) AND arrayExists(x -> {m1}, lp.legs) \
                  AND arrayCount(x -> {m0} OR {m1}, lp.legs) >= 2)"
            ),
            // One bind per `?`, left to right.
            binds: [b0.clone(), b1.clone(), b0.clone(), b1.clone()].concat(),
            params,
        }),
        // `normalize_asset_codes` yields at most two needles; zero means no
        // filter was asked for.
        _ => None,
    }
}

/// One leg matching needle `i`: its asset displays a code containing the
/// needle, or it is one of the needle's Soroban tokens. Returns the
/// expression and its binds; adds the server parameter it reads to `params`.
fn leg_matches(
    i: usize,
    code: &str,
    token_ids: &[i64],
    params: &mut Vec<(String, Vec<i64>)>,
) -> (String, Vec<String>) {
    let by_code = format!(
        "x IN (SELECT id FROM assets WHERE position({}, lower(?)) > 0)",
        crate::common::asset_identity::shown_code_sql(""),
    );
    if token_ids.is_empty() {
        return (by_code, vec![code.to_string()]);
    }
    if token_ids.len() <= MAX_TOKEN_IDS_AS_PARAM {
        params.push((format!("tokens_{i}"), token_ids.to_vec()));
        return (
            format!("({by_code} OR x IN {{tokens_{i}:Array(Int64)}})"),
            vec![code.to_string()],
        );
    }
    (
        format!(
            "({by_code} OR x IN (SELECT id FROM soroban_contracts WHERE contract_id IN ( \
                 SELECT contract_id FROM soroban_contract_metadata FINAL \
                 WHERE positionCaseInsensitive(coalesce(name, ''), ?) > 0 \
                    OR positionCaseInsensitive(coalesce(symbol, ''), ?) > 0)))"
        ),
        vec![code.to_string(), code.to_string(), code.to_string()],
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
