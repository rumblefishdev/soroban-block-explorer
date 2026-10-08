//! Search's asset bucket: an exact `CODE:ISSUER`, or a ranked code substring.

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use super::{IncludeFlags, asset_family_name, asset_route_token};
use crate::search::classifier::Classified;
use crate::search::dto::{EntityType, SearchHit};

// ---------------------------------------------------------------------------
// Assets — asset_code substring (+ native special-case), two-step resolve
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AssetPhase1Row {
    asset_type: i16,
    asset_code: Option<String>,
    /// `soroban_contracts.contract_id` (C-StrKey) via the in-statement join,
    /// `nullIf`-collapsed on a miss.
    contract_strkey: Option<String>,
    /// Surrogate `accounts.id`; `0` = no issuer (native / soroban-native).
    issuer_id: i64,
}

#[derive(Debug, Row, Deserialize)]
struct IssuerRow {
    id: i64,
    account_id: String,
}

/// Fires whenever the query is NOT hash-shaped (a 64-hex / 56-char needle can
/// never be a substring of a ≤12-char asset code, so hash mode is provably
/// empty). Step 1 pages matching assets — `asset_code` substring or the
/// `native`/`xlm` special-case — joining the smaller `soroban_contracts` for the
/// contract StrKey, RANKED by match tier then holder count (task 0485; see the
/// statement comment). Step 2 resolves the page's issuer surrogates → G-StrKey
/// via a bloom-pruned `accounts WHERE id IN (...)` seek (NEVER a full-table
/// `accounts` join — the Code 241 trap). `route_token` is then composed in Rust.
pub(super) async fn search_assets(
    client: &clickhouse::Client,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.asset || classified.hash_bytes.is_some() {
        return Ok(Vec::new());
    }

    // Step 1: page matching assets. `assets` is small (state table); `FINAL`
    // collapses re-ingested versions. `q` is bound (3×: the substring needle and
    // the two case-folded native-token comparisons).
    //
    // The `soroban_contracts` side of the join is collapsed to one row per `id`
    // (`GROUP BY id` + `any(contract_id)`) — NOT joined bare. It is a
    // ReplacingMergeTree with unmerged duplicate ids (some ×6), so a bare join
    // fans each matching asset out 2–6× into identical hits and burns the
    // `per_group_limit` budget. `contract_id` (C-StrKey) is immutable across
    // versions, so `any()` is exact.
    //
    // Grouping the WHOLE (small, ~146k-row) table beats scoping it to the page:
    // a page-scoped `IN (SELECT … FROM page)` needs the `page` CTE, and CH does
    // not materialise CTEs — it would evaluate the asset scan twice. Measured
    // (lore-0420): page-scoped CTE 1,896,766 rows / 37.8 MiB, this form
    // 1,118,154 rows / 28.5 MiB — cheaper even than the un-deduped original
    // (1,151,738 / 32.0 MiB).
    //
    // A fully-qualified `CODE:ISSUER` (task 0534) takes a different arm: the pair
    // names exactly one asset, so it is an equality lookup and needs no ranking —
    // the most precise query is also the cheapest one. The issuer StrKey resolves
    // through `accounts`, whose `ORDER BY account_id` primary key makes it a point
    // seek rather than the ~23M-row hash join that OOMs (Code 241).
    //
    // The substring arm below is where relevance lives (task 0485). Before it,
    // that arm ended in a bare `LIMIT` with NO `ORDER BY`, so it returned
    // whichever rows the scan reached first — `q=USDC` answered with ten `IUSDC`
    // rows and no USDC at all, and two identical calls could disagree.
    //
    // `shown` is the code a row DISPLAYS as, and both the match and the tier
    // compare it — never the stored value. Native XLM stores an EMPTY code and
    // renders as `XLM`, so comparing what is stored returned thousands of
    // impostor codes and missed the one asset everybody meant. That is also why
    // there is no `native` arm: the alias IS the comparison.
    //
    // The same expression appears in `assets::queries` (the list) and in
    // `common::pool_asset_codes` (the legs). It was briefly a shared builder;
    // three literal copies read better than the indirection, so if you change
    // the shape here, change it there — `native_is_matched_by_type_not_by_
    // stored_code` in each module is the test that fails when you do not.
    //
    // Ranking is a tier (exact > prefix > substring anywhere) rather than a
    // scoring formula: the order follows from what matched, not from a weighting
    // we invented. Within a tier the tie-break is holder count — 441 assets carry
    // the code `USDC` and the signal separates them cleanly (691,713 holders for
    // Circle's, 3,093 for the runner-up). It lives in `balance_aggregates` (task
    // 0331) — `assets` has no holder column any more (task 0310). Joined bare:
    // the table is 1:1 on `asset_id`, so the usual `GROUP BY` collapse is pure
    // cost (measured 93 ms -> 71 ms without it), and it cannot be page-scoped
    // like the list's join because the ranking needs holders BEFORE the limit.
    //
    // The trailing PK columns make the order total: holder counts are NULL for
    // most rows, and "same query, same answer" is half of what this fixes.
    //
    // The exact arm deliberately takes none of this: a qualified pair names one
    // row, so there is nothing to rank and nothing to pay the join for.
    const ASSET_HEAD: &str = "SELECT \
            a.asset_type AS asset_type, \
            nullIf(a.asset_code, '') AS asset_code, \
            nullIf(sc.contract_id, '') AS contract_strkey, \
            a.issuer_id AS issuer_id \
         FROM assets a FINAL \
         LEFT JOIN ( \
             SELECT id, any(contract_id) AS contract_id \
             FROM soroban_contracts GROUP BY id \
         ) sc ON sc.id = a.contract_id ";
    let rows = if let Some((code, issuer)) = classified.code_issuer.as_ref() {
        let sql = format!(
            "{ASSET_HEAD} \
             WHERE lower(toString(a.asset_code)) = lower(?) \
               AND a.issuer_id IN (SELECT id FROM accounts WHERE account_id = ?) \
             LIMIT {per_group_limit}"
        );
        client
            .query(&sql)
            .bind(code)
            .bind(issuer)
            .fetch_all::<AssetPhase1Row>()
            .await?
    } else {
        let sql = format!(
            "{ASSET_HEAD} \
             LEFT JOIN balance_aggregates bagg ON bagg.asset_id = a.id \
             WHERE position({shown}, lower(?)) > 0 \
             ORDER BY multiIf({shown} = lower(?), 0, \
                              startsWith({shown}, lower(?)), 1, \
                              2) ASC, \
                 bagg.holder_count DESC NULLS LAST, \
                 a.asset_type ASC, a.asset_code ASC, a.issuer_id ASC \
             LIMIT {per_group_limit}",
            shown = crate::common::asset_identity::shown_code_sql("a."),
        );
        // One bind for the match, two for the tier — left to right, same needle.
        client
            .query(&sql)
            .bind(q)
            .bind(q)
            .bind(q)
            .fetch_all::<AssetPhase1Row>()
            .await?
    };
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: resolve issuer surrogates → G-StrKey. `account_id` is immutable
    // across versions, so no FINAL; `LIMIT 1 BY id` collapses re-ingest parts.
    // `idx_acc_id` (bloom on `id`) prunes the ~23M-row table to the page keys.
    // i64 IN-list, bounded by the page, no injection surface.
    let issuer_ids: BTreeSet<i64> = rows
        .iter()
        .map(|r| r.issuer_id)
        .filter(|&i| i != 0)
        .collect();
    let issuers: HashMap<i64, String> = if issuer_ids.is_empty() {
        HashMap::new()
    } else {
        let in_list = issuer_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        client
            .query(&format!(
                "SELECT id AS id, account_id AS account_id \
                 FROM accounts WHERE id IN ({in_list}) LIMIT 1 BY id"
            ))
            .fetch_all::<IssuerRow>()
            .await?
            .into_iter()
            .map(|r| (r.id, r.account_id))
            .collect()
    };

    Ok(rows
        .into_iter()
        .map(|r| {
            let issuer = (r.issuer_id != 0)
                .then(|| issuers.get(&r.issuer_id))
                .flatten()
                .map(String::as_str);
            let route_token = asset_route_token(
                r.contract_strkey.as_deref(),
                r.asset_code.as_deref(),
                issuer,
                r.asset_type,
            );
            (
                "asset".to_string(),
                SearchHit {
                    entity_type: EntityType::Asset,
                    // Display id is the asset code; native (no code) shows XLM,
                    // matching the PG `COALESCE(asset_code, 'XLM')`.
                    identifier: r.asset_code.unwrap_or_else(|| "XLM".to_string()),
                    label: asset_family_name(r.asset_type).unwrap_or_default(),
                    route_token,
                    successful: None,
                    last_activity_at: None,
                    contract_id: None,
                    token_id: None,
                },
            )
        })
        .collect())
}
