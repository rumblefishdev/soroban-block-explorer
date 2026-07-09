//! ClickHouse queries for the assets endpoints (task 0243).
//!
//! Returns the `AssetRow` shape, so
//! the handler stays backend-agnostic after the fetch. Assets are addressed by
//! the composite identity (contract StrKey / `CODE-ISSUER` / the reserved
//! `native` token); the numeric surrogate was dropped (PR #175), and CH keys
//! `assets` on `(asset_type, asset_code, issuer_id, contract_id)` with no
//! surrogate at all.
//!
//! Notable CH-vs-PG divergences:
//!
//! - **`token_asset_type_name()` is a PG SQL function.** CH has no equivalent,
//!   so the `asset_type` → label mapping is done in Rust ([`asset_type_name`]),
//!   identical to the PG migration `20260422000000_enum_label_functions`.
//! - **`assets a FINAL`** collapses re-ingested asset versions (ReplacingMergeTree
//!   keyed on the 4-tuple). The `soroban_contracts` lookup join carries no FINAL
//!   — the identity columns it projects (contract StrKey, `deployed_at_ledger`)
//!   are write-once / stable across versions. The `accounts` table is NOT joined
//!   (task 0319 for the list, task 0334 for the detail): a full `accounts` hash
//!   side (~18M rows) was the dominant read cost. The issuer is resolved by a
//!   separate `accounts.id` key-seek instead. `home_domain` IS mutable
//!   (SET_OPTIONS), so the seek picks the latest version
//!   (`ORDER BY last_seen_ledger DESC LIMIT 1`); a 16M-row `accounts FINAL`
//!   would be ruinous.
//! - **`nullIf(...)` on the joined columns** (contract_id → `nullIf(_, '')`,
//!   deployed_at_ledger → `nullIf(_, 0)`) makes a LEFT-JOIN miss decode as
//!   `None`. We do NOT use `SETTINGS join_use_nulls = 1` — the `api_reader` CH
//!   user runs under `readonly = 1` (RBAC profile `read_only`), which rejects
//!   per-query setting overrides. Same convention as stkrolikiewicz's CH modules
//!   (accounts `fetch_balances`, LP). The issuer StrKey / home_domain are emptied
//!   to `None` in Rust ([`list_row_to_asset_row`]), not via `nullIf`.
//! - **Cursor** is the composite `AssetKeyCursor`; its keyset clause is only
//!   present on continuation pages, so the clickhouse 0.15 "None bound into a
//!   tuple keyset returns 0 rows" defect is sidestepped (the bound values are
//!   always real). The free-text `asset_code` filter and the cursor values are
//!   `.bind()`-ed (user-controlled); `asset_type` is interpolated (typed `i16`).
//!
//! `/transactions` (canonical 10) keys on the datasource-tagged `TxListCursor`
//! (`Ch { ledger_sequence, tiebreak }`), mirroring the accounts sub-resource.
//! **Read-cost caveat:** the identity predicate (`asset_code`+`asset_issuer_id`
//! / `contract_id`) is a NON-leading-PK filter on `operations_appearances`
//! (ORDER BY `ledger_sequence`), so the driver seek scans by descending ledger
//! until it fills the page — cheap for a hot asset (matches in recent
//! partitions), but a rare asset walks far back. The cursor bounds each page;
//! still, an operator read-rows smoke is required before the prod flag flip
//! (same class as the deferred global tx contract-filter).

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use chrono::{DateTime, Utc};

use crate::common::ch::{self, millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

use super::dto::AssetKeyCursor;

// ---------------------------------------------------------------------------
// Internal query-result rows + helpers (not serialized; the handler maps these
// into the public response DTOs).
// ---------------------------------------------------------------------------

/// Detail/list row for an asset. Handler maps this to the wire `AssetItem`.
#[derive(Debug, Clone)]
pub struct AssetRow {
    pub asset_type: i16,
    /// Pre-decoded via `token_asset_type_name()` SQL helper. `None` only
    /// when the discriminant is outside the schema CHECK range — defensive
    /// against future schema drift.
    pub asset_type_name: Option<String>,
    pub asset_code: Option<String>,
    /// Already resolved through `accounts.account_id` join.
    pub issuer: Option<String>,
    /// Already resolved through `soroban_contracts.contract_id` join.
    pub contract_id: Option<String>,
    pub name: Option<String>,
    /// On-chain SEP-41 token symbol from `soroban_contract_metadata` (task 0297);
    /// `None` for classic/native.
    pub symbol: Option<String>,
    /// Display decimals — on-chain `METADATA` for Soroban tokens, else 7
    /// (Stellar classic precision).
    pub decimals: u32,
    pub total_supply: Option<String>,
    pub holder_count: Option<i32>,
    pub icon_url: Option<String>,
    /// `soroban_contracts.deployed_at_ledger` — populated for SAC and
    /// Soroban-native rows; `None` for native and classic_credit.
    pub deployed_at_ledger: Option<i64>,
    /// `accounts.home_domain` for the issuer, used as the SEP-1 lookup
    /// key in `get_asset` runtime enrichment (task 0188). `None` for
    /// native, no-issuer, and issuer accounts that did not set
    /// `home_domain` on-chain.
    pub issuer_home_domain: Option<String>,
    /// Surrogate key columns — cursor keyset only, never on the wire. These
    /// are the 4-tuple CH orders `assets` by `(asset_type, asset_code,
    /// issuer_id, contract_id)`; `0` / `''` stand in for "absent" (native has
    /// no issuer_id, classic-credit has no contract_id), matching CH defaults.
    pub issuer_id: i64,
    pub contract_surrogate_id: i64,
    /// SAC facet (ADR 0051): the surrogate of the wrapping SAC's `C…` StrKey,
    /// or `0` when the asset has no observed SAC. Never on the wire — the
    /// handler re-derives the display StrKey from `code:issuer` when non-zero.
    pub sac_contract_surrogate: i64,
    /// Whether the `sac_contract_surrogate` SAC is deployed on-chain (ADR 0051).
    pub sac_deployed: bool,
    /// `assets.id` — the `ids::asset_id` surrogate (task 0331). The single asset
    /// key for the `operation_asset_appearances` fan-out seek (task 0359).
    pub id: i64,
}

#[derive(Debug)]
pub struct AssetTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub successful: bool,
    pub fee_charged: i64,
    pub created_at: DateTime<Utc>,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
}

/// Resolved, validated `GET /v1/assets` list params handed to `fetch_list`.
pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<AssetKeyCursor>,
    pub asset_type: Option<i16>,
    /// Raw substring (no `%` / `_` from the caller). The SQL builder
    /// wraps it in `%...%` for the trigram match.
    pub asset_code: Option<String>,
    /// SAC property filter (ADR 0051): restrict to assets with a SAC
    /// (`sac_contract_id != 0`) — the old `filter[type]=sac` view.
    pub sac_only: bool,
}

/// Resolved asset identity used to gate the `/transactions` sub-resource query.
pub struct AssetIdentity<'a> {
    pub asset_code: Option<&'a str>,
    pub issuer: Option<&'a str>,
    pub contract_id: Option<&'a str>,
}

/// True when the asset has a DB-side identity ops can key on. Native XLM and
/// friends have none → the handler short-circuits with an empty page rather
/// than emit a degenerate `WHERE ()`.
pub fn asset_predicate_present(identity: &AssetIdentity<'_>) -> bool {
    let has_classic = identity.asset_code.is_some() && identity.issuer.is_some();
    let has_contract = identity.contract_id.is_some();
    has_classic || has_contract
}

/// `asset_type` SMALLINT → canonical label, matching the PG
/// `token_asset_type_name` function. `None` for an out-of-range code (the PG
/// `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native"),
        1 => Some("classic_credit"),
        // 2 (`sac`) retired — ADR 0051; a SAC is a facet of classic_credit / native.
        3 => Some("soroban"),
        _ => None,
    }
    .map(str::to_string)
}

// Projection / enrichment semantics shared by the list + detail reads (task
// 0334 collapsed the detail paths onto the same accounts-join-free SELECT).
// Column order MUST match the row struct (positional decode); the `nullIf(...)`
// wraps make a LEFT-JOIN miss decode as `None` (readonly-safe; no
// `SETTINGS join_use_nulls`).
// Enrichment (icon_url + classic/SAC name) is read from the `asset_enrichment`
// side table (ADR 0050 / task 0231), NOT the indexer-owned `assets.{icon_url,
// name}` placeholders (dropped, task 0231 step 8). Per Option C the name has a
// single owner per `asset_type`, composed disjointly at read:
//   classic/SAC (1,2) → `asset_enrichment.name`
//   soroban (3)       → `soroban_contract_metadata.name` (on-chain instance
//                       `METADATA` struct; the legacy `soroban_contracts.name`
//                       column is dead — no writer since task 0297)
//   native (0)        → the `"Stellar Lumens"` literal
// `asset_enrichment` is `ReplacingMergeTree(version)`; the `argMax(_, version)`
// sub-aggregate collapses it to one latest row per key so the LEFT JOIN can't
// multiply asset rows on un-merged duplicates. `''` is the sentinel
// ("tried, nothing"), neutralised with `nullIf`.
// `total_supply` / `holder_count` come from `balance_aggregates` (task 0331,
// Option A) — the single pre-computed aggregate over the unified `balances`
// table, keyed by the `assets.id` surrogate (`bagg.asset_id = a.id`), one 1:1
// LEFT JOIN for ALL asset types. `total_supply` = Σ per-holder `amount` over G+C
// holders: a mint always credits a holder balance (often a contract treasury,
// summed because we sum contracts too), so the sum equals the token's real
// supply; the narrow residue (TTL-archived tail + true rebasing) is the accepted
// non-100% cost of one universal method — no per-token `TotalSupply` key read
// (see the task 0331 Option-A decision). RAW `Int128` (the API returns it raw;
// clients scale by `decimals`, classic = 7). `Nullable` columns, so a JOIN miss
// (no holders — reads NULL under the readonly `api_reader`, where
// `join_use_nulls = 0` defaults a Nullable to NULL) renders "—", not a fake 0.
// Refreshed by the MV on a cadence (eventually consistent). Requires `assets.id`
// populated (prod: ALTER + backfill — see init.sql).
// NOTE: post-ADR-0051 (task 0339, merged) a SAC is a FACET of its classic/native
// asset, not a separate `assets` row (type-2 retired). Contract-held SAC balances are
// re-keyed onto the WRAPPED classic id in `build_balance_rows`, so they sum INTO that
// classic asset's `balance_aggregates` row — ONE unified supply per asset. (Replaces
// the retired `asset_aggregates`, which keyed `(asset_code, issuer_id)`.)
/// Accounts-join-free SELECT, shared by the list (task 0319) AND the detail
/// paths (task 0334). It drops the `LEFT JOIN accounts iss` (and the two `issuer`
/// columns it produced) that built its hash side from the whole `accounts` table
/// (~18M rows on prod) — the dominant cost of both the ~1.35s `/assets` list and
/// the ~1s `/assets/:id` detail (prod: ~21M read_rows / ~1.58 GB / request, of
/// which `accounts` was ~18.5M). The issuer StrKey + home_domain are resolved by
/// a bloom-pruned `accounts.id` key-seek instead — per-page for the list, single
/// for the detail (see [`resolve_issuer`]).
const ASSET_LIST_CH_SELECT: &str = "SELECT \
     a.asset_type                 AS asset_type, \
     nullIf(a.asset_code, '')     AS asset_code, \
     nullIf(sc.contract_id, '')   AS contract_id, \
     coalesce(nullIf(ae.name, ''), nullIf(m.name, ''), \
              if(a.asset_type = 0, 'Stellar Lumens', NULL)) AS name, \
     nullIf(m.symbol, '')         AS symbol, \
     coalesce(m.decimals, 7)      AS decimals, \
     toString(bagg.total_supply)  AS total_supply, \
     bagg.holder_count            AS holder_count, \
     nullIf(coalesce(nullIf(sc.deployed_at_ledger, 0), \
                     nullIf(sac_sc.deployed_at_ledger, 0)), 0) AS deployed_at_ledger, \
     nullIf(ae.icon_url, '')      AS icon_url, \
     a.issuer_id                  AS issuer_id_key, \
     a.contract_id                AS contract_id_key, \
     sac.sac_contract_id          AS sac_contract_surrogate, \
     sac.sac_deployed             AS sac_deployed, \
     a.id                         AS id \
     FROM assets a FINAL \
     LEFT JOIN soroban_contracts sc  ON sc.id  = a.contract_id \
     LEFT JOIN ( \
         SELECT contract_id, name, symbol, decimals \
         FROM soroban_contract_metadata FINAL \
     ) m ON m.contract_id = sc.contract_id \
     LEFT JOIN balance_aggregates bagg ON bagg.asset_id = a.id \
     LEFT JOIN ( \
         SELECT asset_type, asset_code, issuer_id, contract_id, \
                argMax(icon_url, version) AS icon_url, \
                argMax(name, version)     AS name \
         FROM asset_enrichment \
         GROUP BY asset_type, asset_code, issuer_id, contract_id \
     ) ae ON ae.asset_type  = a.asset_type  AND ae.asset_code  = a.asset_code \
         AND ae.issuer_id   = a.issuer_id   AND ae.contract_id = a.contract_id \
     LEFT JOIN ( \
         SELECT asset_type, asset_code, issuer_id, contract_id, \
                max(sac_contract_id)        AS sac_contract_id, \
                toBool(max(sac_deployed))   AS sac_deployed \
         FROM asset_sac \
         GROUP BY asset_type, asset_code, issuer_id, contract_id \
     ) sac ON sac.asset_type  = a.asset_type  AND sac.asset_code  = a.asset_code \
         AND sac.issuer_id   = a.issuer_id   AND sac.contract_id = a.contract_id \
     LEFT JOIN soroban_contracts sac_sc \
         ON sac_sc.id = sac.sac_contract_id AND sac.sac_contract_id != 0";

/// Row decoded from [`ASSET_LIST_CH_SELECT`] — the asset header WITHOUT the
/// join-resolved `issuer` / `issuer_home_domain`, which are key-seeked separately
/// (per-page for the list, task 0319; per-request for the detail, task 0334).
#[derive(Debug, Row, Deserialize)]
struct AssetListChRow {
    asset_type: i16,
    asset_code: Option<String>,
    contract_id: Option<String>,
    name: Option<String>,
    symbol: Option<String>,
    decimals: u32,
    total_supply: Option<String>,
    holder_count: Option<i32>,
    deployed_at_ledger: Option<i64>,
    icon_url: Option<String>,
    issuer_id_key: i64,
    contract_id_key: i64,
    sac_contract_surrogate: i64,
    sac_deployed: bool,
    id: i64,
}

/// Issuer resolve row: `accounts` → `id` (surrogate) + `account_id` StrKey +
/// `home_domain`. Decoded from [`seek_latest_account`], reached two ways: by `id`
/// (rides the `idx_acc_id` bloom skip-index — list per-page seek, task 0319, and
/// the detail `resolve_issuer`, task 0334) or by `account_id` (the table's
/// `ORDER BY account_id` primary key — the CODE-ISSUER detail form, task 0334).
#[derive(Debug, Row, Deserialize)]
struct AssetIssuerRow {
    id: i64,
    account_id: String,
    home_domain: Option<String>,
}

/// Build an [`AssetRow`] from an [`ASSET_LIST_CH_SELECT`] projection row plus the
/// separately key-seeked issuer (`(StrKey, home_domain)`), consumed by value (no
/// clone). `iss` is `None` for native / no-issuer assets or a seek miss. Shared
/// by the list page map and the three detail fetches (task 0334).
fn list_row_to_asset_row(r: AssetListChRow, iss: Option<(String, Option<String>)>) -> AssetRow {
    let (issuer, issuer_home_domain) = match iss {
        Some((account_id, home_domain)) => (
            Some(account_id).filter(|s| !s.is_empty()),
            home_domain.filter(|s| !s.is_empty()),
        ),
        None => (None, None),
    };
    AssetRow {
        asset_type: r.asset_type,
        asset_type_name: asset_type_name(r.asset_type),
        asset_code: r.asset_code,
        issuer,
        contract_id: r.contract_id,
        name: r.name,
        symbol: r.symbol,
        decimals: r.decimals,
        total_supply: r.total_supply,
        holder_count: r.holder_count,
        icon_url: r.icon_url,
        deployed_at_ledger: r.deployed_at_ledger,
        issuer_home_domain,
        issuer_id: r.issuer_id_key,
        contract_surrogate_id: r.contract_id_key,
        sac_contract_surrogate: r.sac_contract_surrogate,
        sac_deployed: r.sac_deployed,
        id: r.id,
    }
}

/// Single-row `accounts` seek → ([`AssetIssuerRow`]: `id`, `account_id` StrKey,
/// `home_domain`), latest version. `where_col` MUST be a trusted static column
/// name — `"id"` (rides the `idx_acc_id` bloom skip-index) or `"account_id"`
/// (the `ORDER BY account_id` primary key) — NEVER user input (it is
/// interpolated, the value is bound). `home_domain` is mutable (SET_OPTIONS), so
/// pick the latest version (`ORDER BY last_seen_ledger DESC LIMIT 1`, no FINAL).
async fn seek_latest_account(
    client: &clickhouse::Client,
    where_col: &str,
    value: impl serde::Serialize,
) -> Result<Option<AssetIssuerRow>, clickhouse::error::Error> {
    let sql = format!(
        "SELECT id AS id, account_id AS account_id, home_domain AS home_domain \
         FROM accounts WHERE {where_col} = ? \
         ORDER BY last_seen_ledger DESC LIMIT 1"
    );
    client
        .query(&sql)
        .bind(value)
        .fetch_optional::<AssetIssuerRow>()
        .await
}

/// Resolve one issuer surrogate `id` → `(StrKey, home_domain)` via the
/// `accounts.id` bloom-pruned key-seek (task 0334) — NOT a full `accounts` scan /
/// hash join. `id == 0` (native / no issuer) returns `None` without a query.
async fn resolve_issuer(
    client: &clickhouse::Client,
    issuer_id: i64,
) -> Result<Option<(String, Option<String>)>, clickhouse::error::Error> {
    if issuer_id == 0 {
        return Ok(None);
    }
    Ok(seek_latest_account(client, "id", issuer_id)
        .await?
        .map(|r| (r.account_id, r.home_domain)))
}

/// Finish a detail fetch whose asset row was read from [`ASSET_LIST_CH_SELECT`]:
/// resolve the issuer by an `accounts.id` key-seek, then map to [`AssetRow`].
/// Shared by the contract-StrKey and native forms (the CODE-ISSUER form resolves
/// its issuer up front, so it maps directly). A `None` row ⇒ asset not found.
async fn finish_detail(
    client: &clickhouse::Client,
    row: Option<AssetListChRow>,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let Some(row) = row else {
        return Ok(None);
    };
    let iss = resolve_issuer(client, row.issuer_id_key).await?;
    Ok(Some(list_row_to_asset_row(row, iss)))
}

// ---------------------------------------------------------------------------
// List — GET /v1/assets (canonical 08)
// ---------------------------------------------------------------------------

/// **Read-cost caveat:** the keyset/`ORDER BY` is the identity 4-tuple — which
/// IS the `assets` primary key — so the page is a PK-prefix walk, cheap. The
/// `FINAL` + lookup joins widen it; needs an operator smoke before the prod
/// flag flip, same as the other CH lists.
pub async fn fetch_list(
    client: &clickhouse::Client,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<AssetRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    let type_clause = params
        .asset_type
        .map_or_else(String::new, |t| format!(" AND a.asset_type = {t}"));
    // SAC property filter (ADR 0051): the old `filter[type]=sac` view, now a
    // facet predicate over the joined `asset_sac` side table. Deployed-only —
    // `sac_deployed` excludes reserved (un-deployed) SAC addresses, which are
    // not live contracts (task 0339 UX). A LEFT-JOIN miss decodes as 0/false
    // under the readonly `join_use_nulls = 0`.
    let sac_clause = if params.sac_only {
        " AND sac.sac_deployed"
    } else {
        ""
    };
    let code_clause = if params.asset_code.is_some() {
        " AND positionCaseInsensitive(a.asset_code, ?) > 0"
    } else {
        ""
    };
    let cursor_clause = if params.cursor.is_some() {
        format!(" AND (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) {op} (?, ?, ?, ?)")
    } else {
        String::new()
    };

    let sql = format!(
        "{ASSET_LIST_CH_SELECT} \
         WHERE 1{type_clause}{sac_clause}{code_clause}{cursor_clause} \
         ORDER BY a.asset_type {order}, a.asset_code {order}, \
                  a.issuer_id {order}, a.contract_id {order} \
         LIMIT ?"
    );

    let mut query = client.query(&sql);
    if let Some(code) = &params.asset_code {
        query = query.bind(code);
    }
    if let Some(c) = &params.cursor {
        query = query
            .bind(c.asset_type)
            .bind(&c.asset_code)
            .bind(c.issuer_id)
            .bind(c.contract_id);
    }
    // `params.limit` is the handler's `fetch_limit()` (already the peek +1).
    let rows = query
        .bind(params.limit)
        .fetch_all::<AssetListChRow>()
        .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Resolve the page's issuer surrogates → (StrKey, home_domain) by a
    // bloom-pruned key-seek (`accounts.idx_acc_id`), replacing the full-table
    // `accounts iss` join (task 0319). `issuer_id = 0` (e.g. native XLM) ⇒ no
    // issuer. i64 IN-list, bounded by the page limit, no injection surface.
    let issuers: std::collections::HashMap<i64, (String, Option<String>)> = {
        let ids = rows
            .iter()
            .map(|r| r.issuer_id_key)
            .filter(|&i| i != 0)
            .collect::<BTreeSet<_>>();
        if ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            let in_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            client
                .query(&format!(
                    // `ORDER BY last_seen_ledger DESC LIMIT 1 BY id`: `accounts`
                    // is ReplacingMergeTree(last_seen_ledger) and we read without
                    // FINAL, so pick the latest version per id deterministically
                    // — `home_domain` is mutable (SET_OPTIONS), so an arbitrary
                    // version would be non-deterministic (review 0319).
                    "SELECT id AS id, account_id AS account_id, home_domain AS home_domain \
                     FROM accounts WHERE id IN ({in_list}) \
                     ORDER BY last_seen_ledger DESC LIMIT 1 BY id"
                ))
                .fetch_all::<AssetIssuerRow>()
                .await?
                .into_iter()
                .map(|r| (r.id, (r.account_id, r.home_domain)))
                .collect()
        }
    };

    Ok(rows
        .into_iter()
        .map(|r| {
            let iss = issuers.get(&r.issuer_id_key).cloned();
            list_row_to_asset_row(r, iss)
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Detail — GET /v1/assets/:id (canonical 09), three resolution forms
// ---------------------------------------------------------------------------

/// Resolve by contract StrKey (`C…`) — either a bespoke `soroban` asset (the
/// contract IS the asset) OR a SAC whose deep-link must resolve to the wrapped
/// classic / native asset (ADR 0051). Two match arms:
///   * `sc.contract_id = ?` — soroban identity (the `assets.contract_id` key,
///     resolved via the `soroban_contracts` join).
///   * `sac.sac_contract_id = {surrogate}` — the SAC facet (joined `asset_sac`).
///     The `C…` StrKey is hashed to its `asset_sac.sac_contract_id` surrogate the
///     same way the writer derives it (`db_clickhouse::persist::ids::contract_id`),
///     so `/assets/{C…}` for a SAC lands on its classic / native row.
///
/// A `C…` is at most one of the two (a SAC address ≠ a deployed-wasm address),
/// so the OR yields a single row. Issuer StrKey + home_domain then resolve by a
/// single `accounts.id` key-seek (task 0334).
pub async fn fetch_by_contract_id(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let sac_surrogate = db_clickhouse::persist::ids::contract_id(contract_id);
    let sql = format!(
        "{ASSET_LIST_CH_SELECT} \
         WHERE sc.contract_id = ? OR sac.sac_contract_id = {sac_surrogate} LIMIT 1"
    );
    let row = client
        .query(&sql)
        .bind(contract_id)
        .fetch_optional::<AssetListChRow>()
        .await?;
    finish_detail(client, row).await
}

/// Resolve by `CODE-ISSUER` (classic credit / classic-wrap SAC). `issuer` is the
/// G-StrKey. Resolve it to the surrogate `issuer_id` first via the
/// `accounts.account_id` PK seek (accounts is `ORDER BY account_id`), then filter
/// `assets` by `(asset_code, issuer_id)` — no full `accounts` join (task 0334).
/// The same seek yields the issuer StrKey + home_domain, so no second lookup.
/// `ORDER BY a.asset_type` is a deterministic tiebreak; post-ADR 0051 a
/// `(code, issuer)` maps to a single classic_credit row (its SAC is a facet on
/// that same row, not a separate type=2), so at most one row matches anyway.
pub async fn fetch_by_code_issuer(
    client: &clickhouse::Client,
    asset_code: &str,
    issuer: &str,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let Some(issuer_row) = seek_latest_account(client, "account_id", issuer).await? else {
        return Ok(None);
    };

    let sql = format!(
        "{ASSET_LIST_CH_SELECT} \
         WHERE a.asset_code = ? AND a.issuer_id = ? \
         ORDER BY a.asset_type LIMIT 1"
    );
    let Some(row) = client
        .query(&sql)
        .bind(asset_code)
        .bind(issuer_row.id)
        .fetch_optional::<AssetListChRow>()
        .await?
    else {
        return Ok(None);
    };

    let iss = (issuer_row.account_id, issuer_row.home_domain);
    Ok(Some(list_row_to_asset_row(row, Some(iss))))
}

/// Resolve the classic native XLM singleton (`asset_type = 0`). Native has no
/// composite identity, so it is addressed by the reserved `/assets/native`
/// token rather than a StrKey or CODE-ISSUER pair. `issuer_id = 0` → no issuer
/// seek (task 0334).
pub async fn fetch_native(
    client: &clickhouse::Client,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let sql = format!("{ASSET_LIST_CH_SELECT} WHERE a.asset_type = 0 LIMIT 1");
    let row = client
        .query(&sql)
        .fetch_optional::<AssetListChRow>()
        .await?;
    finish_detail(client, row).await
}

// ---------------------------------------------------------------------------
// Transactions — GET /v1/assets/:id/transactions (canonical 10, two-step)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AssetTxKeyChRow {
    ledger_sequence: i64,
    transaction_id: i64,
}

#[derive(Debug, Row, Deserialize)]
struct AssetTxPageChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    source_id: i64,
    fee_charged: i64,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at: i64,
}

/// Per-`asset_type` predicate composition over `operations_appearances`,
/// mirroring the PG `fetch_transactions` branches but on CH surrogate ids:
///   classic_credit / classic-wrap SAC → `(asset_code, asset_issuer_id)`
///   native-wrap SAC / soroban          → `contract_id`
///   native XLM (no identity)           → caller short-circuits (empty page)
///
/// `asset_issuer_id` / `contract_surrogate_id` are the surrogate keys carried
/// on the resolved `AssetRow` (`0` = absent), so no StrKey→id lookup is needed.
/// Two-step like the accounts sub-resource: a driver seek collapses any
/// multi-op-per-tx fan-out (`LIMIT 1 BY (ledger_sequence, transaction_id)`),
/// then the ≤`limit` transaction headers are fetched by
/// `(ledger_sequence, id) IN (keys)` (PK-prefix prune, multi-partition-safe)
/// and the `operation_types` aggregate is merged. The caller passes the
/// handler's `fetch_limit()` (already the `+1` `finalize_page` peek row), bound
/// raw — same convention as the PG `fetch_transactions`.
pub async fn fetch_transactions(
    client: &clickhouse::Client,
    asset_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<AssetTxRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // CH cursor only; a `Pg` variant never reaches here (the handler's
    // cross-source guard rejects it). Inlined i64 — no injection surface; the
    // clause is omitted entirely on the first page so no NULL is bound.
    let cursor_clause = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => format!(
            " AND (p.ledger_sequence, p.transaction_id) {op} ({ledger_sequence}, {tiebreak})"
        ),
        _ => String::new(),
    };

    // Step 1: leading-key seek over `operation_asset_appearances`. `asset_id` IS
    // the leading sort key, so this is a bounded PK-prefix range read — not the
    // old non-leading density-scan on `operations_appearances` that timed out
    // (the perf fix for this endpoint IS the fan-out — asttxs convergence).
    // `LIMIT 1 BY (ledger, tx)` collapses per-op fan-out to one row per tx on the
    // asset-contiguous range (does NOT defeat read-in-order here). The
    // `max(sequence)` fence keeps the seek behind the ledgers commit marker, so a
    // head key whose tx header isn't written yet cannot truncate the page.
    let driver_sql = format!(
        "SELECT p.ledger_sequence AS ledger_sequence, p.transaction_id AS transaction_id \
         FROM operation_asset_appearances p \
         WHERE p.asset_id = {asset_id} \
           AND p.ledger_sequence <= (SELECT max(sequence) FROM ledgers){cursor_clause} \
         ORDER BY p.ledger_sequence {order}, p.transaction_id {order} \
         LIMIT 1 BY p.ledger_sequence, p.transaction_id \
         LIMIT {limit}"
    );
    let raw = client
        .query(&driver_sql)
        .fetch_all::<AssetTxKeyChRow>()
        .await?;
    let keys: Vec<(i64, i64)> = raw
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

    if keys.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: transaction headers for the page keys + the operation_types
    // aggregate, concurrently. Keys are i64 — inlined.
    let in_tuples = keys
        .iter()
        .map(|(ledger, tx)| format!("({ledger},{tx})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = keys
        .iter()
        .map(|(ledger, _)| ledger / 500_000)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let page_sql = format!(
        "SELECT \
            t.id AS id, \
            lower(hex(t.hash)) AS hash, \
            t.ledger_sequence AS ledger_sequence, \
            t.source_id AS source_id, \
            t.fee_charged AS fee_charged, \
            t.successful AS successful, \
            t.operation_count AS operation_count, \
            t.has_soroban AS has_soroban, \
            l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions})"
    );
    let (page_rows, aggregates) = tokio::join!(
        client.query(&page_sql).fetch_all::<AssetTxPageChRow>(),
        ch::fetch_tx_list_aggregates(client, &keys),
    );
    let page_rows = page_rows?;
    let aggregates = aggregates?;
    // Resolve source StrKeys by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN accounts src` (task 0354).
    let accounts =
        resolve_accounts(client, page_rows.iter().map(|r| r.source_id).collect()).await?;

    // Index by id, then emit in the driver's keyset order, merging
    // operation_types. `contract_ids` from the helper is intentionally unused —
    // an asset-transaction item carries only `operation_types`.
    let mut by_id: HashMap<i64, AssetTxPageChRow> = HashMap::with_capacity(page_rows.len());
    for row in page_rows {
        by_id.insert(row.id, row);
    }
    let mut out = Vec::with_capacity(keys.len());
    for (_, tx_id) in &keys {
        let Some(row) = by_id.remove(tx_id) else {
            continue;
        };
        let operation_types = aggregates
            .get(tx_id)
            .map(|a| a.operation_types.clone())
            .unwrap_or_default();
        out.push(AssetTxRow {
            id: row.id,
            hash: row.hash,
            ledger_sequence: row.ledger_sequence,
            source_account: accounts
                .get(&row.source_id)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_default(),
            successful: row.successful,
            fee_charged: row.fee_charged,
            created_at: millis_to_utc(row.created_at),
            operation_count: row.operation_count,
            has_soroban: row.has_soroban,
            operation_types,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_type_name_matches_pg_function() {
        assert_eq!(asset_type_name(0).as_deref(), Some("native"));
        assert_eq!(asset_type_name(1).as_deref(), Some("classic_credit"));
        // 2 (`sac`) retired — ADR 0051.
        assert_eq!(asset_type_name(2), None);
        assert_eq!(asset_type_name(3).as_deref(), Some("soroban"));
        assert_eq!(asset_type_name(99), None);
    }
}
