//! ClickHouse queries for the contracts endpoints (task 0243).
//!
//! Returns the same response shapes across list/detail, so the handler
//! stays backend-agnostic after the fetch. Contracts are addressed by the
//! natural C-StrKey throughout (no numeric surrogate on the wire), so there is
//! no asset-style id-mapping problem.
//!
//! `events` (canonical 14) reads CH `soroban_events` directly — the full-content
//! per-event table (ADR 0044 §4a). `topics_xdr` / `data_xdr` are a misnomer: the
//! indexer already ScVal-decodes them to JSON at ingest (`persist::stage`), and
//! also drops diagnostic-source events there, so the CH read path just
//! JSON-deserializes the inline payload — NO Archive S3 overlay, NO read-time
//! XDR decode (the PG path's `expand_events`). Keyset is 3-component
//! `(ledger_sequence, transaction_id, event_index)` (`event_index` = the
//! multi-event-tx tie-break). Pagination unit differs from PG: CH pages per
//! EVENT (one row → one `EventItem`), PG pages per folded APPEARANCE (one row →
//! many events, expanded at read time). The PG fold-count is an internal
//! storage detail and is not surfaced on the wire.
//!
//! Read-cost notes (lessons from the global tx-list firefight):
//! - `soroban_contracts` is ORDER BY `(contract_id)`; `soroban_invocations_appearances`
//!   is ORDER BY `(contract_id, ledger_sequence, transaction_id)`. So every
//!   filter by `contract_id` is a LEADING-primary-key seek, never a scan.
//! - A contract's invocations span MANY ledger partitions, so (like account
//!   transactions) the page is driven off the seek, then the ≤limit transaction
//!   rows are fetched by `(ledger_sequence, id) IN (keys)` and merged in Rust —
//!   never an unpruned `transactions FINAL` join (the read_rows-quota trap).

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use domain::ContractEventType;

use crate::common::ch::{millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

use chrono::{DateTime, Utc};

use super::dto::ContractIdCursor;
use super::dto::{ContractStats, EventCursor, EventItem, SacAsset};

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

/// Recent-activity window shared by the detail stats (`fetch_contract_stats`)
/// and the list's `recent_invocations` column, so both compute the count over
/// the SAME period. Single source — they cannot drift.
pub(crate) const STATS_WINDOW_DAYS: i64 = 7;

/// Wire label for [`STATS_WINDOW_DAYS`], surfaced as `stats_window` on the
/// contract-stats DTO. Derived FROM the number rather than parsed back INTO it:
/// the window is a compile-time constant with no user input, so a string
/// source-of-truth only created a parse that cannot fail and a fallback that
/// cannot run (lore-0420).
pub(crate) fn stats_window_label() -> String {
    format!("{STATS_WINDOW_DAYS} days")
}

#[derive(Debug)]
pub struct ContractRow {
    pub id: i64,
    pub contract_id: String,
    pub wasm_hash: Option<String>,
    pub wasm_uploaded_at_ledger: Option<i64>,
    pub deployer: Option<String>,
    pub deployed_at_ledger: Option<i64>,
    pub contract_type_name: Option<String>,
    pub contract_type: Option<i16>,
    pub is_sac: bool,
    /// Task 0441 — the asset this SAC mirrors; `None` unless `is_sac`.
    pub sac_asset: Option<SacAsset>,
    /// Task 0327 — contract mutability, 3-state (`None` = Unknown).
    pub upgradeable: Option<bool>,
}

#[derive(Debug)]
pub struct ContractListRow {
    pub id: i64,
    pub contract_id: String,
    pub contract_type: Option<i16>,
    pub contract_type_name: Option<String>,
    pub is_sac: bool,
    /// Task 0441 — the asset this SAC mirrors; `None` unless `is_sac`.
    pub sac_asset: Option<SacAsset>,
    pub deployer: Option<String>,
    pub deployed_at_ledger: Option<i64>,
    pub recent_invocations: i64,
}

/// Resolved, validated `GET /v1/contracts` list params.
pub struct ResolvedContractsListParams {
    pub limit: i64,
    pub cursor: Option<ContractIdCursor>,
    pub contract_type: Option<i16>,
    /// Free-text search; matched against `search_vector` (name + contract_id).
    pub q: Option<String>,
}

#[derive(Debug)]
pub struct InterfaceRow {
    pub contract_id: String,
    pub wasm_hash: Option<String>,
    /// `None` for SAC / pre-upload / stub rows.
    pub interface_metadata: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct InvocationAppearanceRow {
    pub transaction_id: i64,
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
    pub caller_account: Option<String>,
    pub successful: bool,
}

/// `contract_type` SMALLINT → label, matching the PG `contract_type_name`
/// function (migration `20260422000100_contract_type_add_nft_fungible`).
/// `None` for an out-of-range code (PG `CASE` returns NULL).
fn contract_type_name(contract_type: i16) -> Option<String> {
    match contract_type {
        0 => Some("token".to_string()),
        1 => Some("other".to_string()),
        2 => Some("nft".to_string()),
        3 => Some("fungible".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// List — GET /v1/contracts
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ContractListChRow {
    id: i64,
    contract_id: String,
    contract_type: Option<i16>,
    is_sac: bool,
    /// `accounts` surrogate of the deployer (`Nullable(Int64)`); resolved to the
    /// StrKey in step 2 (task 0319). `NULL` / `0` / no match ⇒ no deployer.
    deployer_id: Option<i64>,
    deployed_at_ledger: Option<i64>,
}

#[derive(Debug, Row, Deserialize)]
struct InvocationCountChRow {
    contract_id: i64,
    recent_invocations: u64,
}

#[derive(Debug, Row, Deserialize)]
struct SacAssetChRow {
    sac_contract_id: i64,
    asset_type: i16,
    asset_code: String,
    issuer_id: i64,
}

/// Reverse SAC lookup (task 0441): the page's SAC contract surrogates → the
/// classic asset each one mirrors, shared by the list and the detail so both
/// resolve identically.
///
/// ONE batched aggregation per page, never per row — `asset_sac` is sorted by
/// the ASSET side, so any lookup by `sac_contract_id` reads the whole table
/// (7.79 MiB / ~0.1 s measured on prod, decision recorded in the task; a
/// `bloom_filter` skip index is the named upgrade past ~5M rows). The
/// `GROUP BY` collapses the AggregatingMergeTree's multi-row facet (up to 7
/// rows per contract, identity-constant so `max` is safe); `FINAL` is not
/// used on Aggregating engines in this codebase — every reader collapses via
/// GROUP BY (mirrors the LP `sac` subquery).
///
/// Native XLM is `asset_type = 0` — NOT an empty code or zero issuer, which
/// classic rows can never carry but which are not the native *signal* (two
/// competing native conventions exist in this codebase). Classic issuers
/// resolve via the same bloom key-seek as the deployer column; a SAC whose
/// code or issuer cannot be resolved is omitted from the map, so the caller
/// falls back to the bare `is_sac` badge rather than a half-identity.
async fn fetch_sac_assets(
    client: &clickhouse::Client,
    sac_ids: &[i64],
) -> Result<HashMap<i64, SacAsset>, clickhouse::error::Error> {
    if sac_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_list = sac_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let rows = client
        .query(&format!(
            "SELECT \
                sac_contract_id  AS sac_contract_id, \
                max(asset_type)  AS asset_type, \
                max(asset_code)  AS asset_code, \
                max(issuer_id)   AS issuer_id \
             FROM asset_sac \
             WHERE sac_contract_id IN ({in_list}) \
             GROUP BY sac_contract_id"
        ))
        .fetch_all::<SacAssetChRow>()
        .await?;

    let issuers = resolve_accounts(
        client,
        rows.iter()
            .filter(|r| r.asset_type != 0 && r.issuer_id != 0)
            .map(|r| r.issuer_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    )
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|r| {
            if r.asset_type == 0 {
                return Some((
                    r.sac_contract_id,
                    SacAsset {
                        asset_code: None,
                        issuer: None,
                    },
                ));
            }
            let issuer = issuers.get(&r.issuer_id).filter(|s| !s.is_empty())?;
            if r.asset_code.is_empty() {
                return None;
            }
            Some((
                r.sac_contract_id,
                SacAsset {
                    asset_code: Some(r.asset_code),
                    issuer: Some(issuer.clone()),
                },
            ))
        })
        .collect())
}

/// CH equivalent of the PG `queries::fetch_contract_list` (task 0275). Same
/// response shape + same opaque cursor (`ContractIdCursor{id}` — `id` exists on
/// CH `soroban_contracts`, so the cursor is datasource-agnostic).
///
/// Two-step, mirroring the account-tx / invocations CH pattern:
///
/// 1. Page the contracts (PK-collapsed via `FINAL`), keyset on `id DESC`.
/// 2. For the page's ≤limit surrogate ids, count invocations in the
///    `STATS_WINDOW` and merge `recent_invocations` in Rust — same table +
///    predicate + window as the detail's `fetch_contract_stats`, so a list
///    item's count matches the detail (PG parity invariant). A per-row
///    correlated subquery (the PG shape) does not translate to CH.
///
/// Divergences from PG:
///
/// - **`filter[q]`** has no CH equivalent for PG's `search_vector` FTS
///   (`websearch_to_tsquery`). Fallback = case-insensitive substring on
///   `contract_id` only (`positionCaseInsensitive`). A full scan of the
///   (small) contracts table, NOT tokenized search — close enough for the
///   explorer's id lookup, documented divergence. (The legacy `sc.name` arm
///   was dropped in task 0304 — empty since 0297, no name to search.)
/// - **`contract_type` / cursor** are interpolated (typed `i16` / `i64`, no
///   injection surface); the free-text `q` is `.bind()`-ed.
///
/// **Read-cost caveat:** the sort is `id DESC`, NOT the `soroban_contracts`
/// primary key (`contract_id`), so step 1 scans the table to order it. The
/// table is far smaller than `transactions` / `accounts`; acceptable, but worth
/// an operator smoke before the prod flag flip.
pub async fn fetch_contract_list(
    client: &clickhouse::Client,
    params: &ResolvedContractsListParams,
    direction: Direction,
) -> Result<Vec<ContractListRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Keyset clause omitted entirely on the first page (no cursor) — the unified
    // CH-list convention — so the clickhouse 0.15 "None into a tuple keyset → 0
    // rows" defect can never fire. The i64 cursor id is inlined (no injection).
    let cursor_clause = params
        .cursor
        .as_ref()
        .map_or_else(String::new, |c| format!(" AND sc.id {op} {}", c.id));
    let type_clause = params
        .contract_type
        .map_or_else(String::new, |t| format!(" AND sc.contract_type = {t}"));
    let q_clause = if params.q.is_some() {
        // Substring on `contract_id` only. The legacy `sc.name` arm was dropped
        // (task 0304): `soroban_contracts.name` has had no writer since 0297 and
        // is empty in prod, so the OR never matched — and the contract API does
        // not surface a name to search against (0297 #3).
        " AND positionCaseInsensitive(sc.contract_id, ?) > 0"
    } else {
        ""
    };

    // Deployer is NOT joined here (task 0319): `LEFT JOIN accounts` built the
    // hash side from the whole `accounts` table (~18M rows on prod) — the
    // reverse-id lookup is resolved per-page below by a bloom-pruned key-seek.
    let list_sql = format!(
        "SELECT \
            sc.id                           AS id, \
            sc.contract_id                  AS contract_id, \
            sc.contract_type                AS contract_type, \
            sc.is_sac                       AS is_sac, \
            sc.deployer_id                  AS deployer_id, \
            sc.deployed_at_ledger           AS deployed_at_ledger \
         FROM soroban_contracts sc FINAL \
         WHERE 1{cursor_clause}{type_clause}{q_clause} \
         ORDER BY sc.id {order} \
         LIMIT ?"
    );

    let mut list_query = client.query(&list_sql);
    if let Some(q) = &params.q {
        list_query = list_query.bind(q);
    }
    // `params.limit` is the handler's `fetch_limit()` (already the peek +1).
    let list_rows = list_query
        .bind(params.limit)
        .fetch_all::<ContractListChRow>()
        .await?;

    if list_rows.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: invocation counts in the STATS_WINDOW for the page's ids.
    //
    // The window is ONE `ledger_sequence >= (first sequence in the window)`
    // bound, resolved from the data (lore-0420). That single expression does
    // three jobs the earlier shapes needed three constructs for:
    //   * it is exact — no constant guessing how many ledgers fit in a day;
    //   * it keeps the seek on the `(contract_id, ledger_sequence)` PK prefix;
    //   * it cannot fan out. `ledgers` is a ReplacingMergeTree with unmerged
    //     duplicate rows, and JOINing it multiplied every appearance row per
    //     duplicate copy (measured ~1.6× inflation). `min()` is immune to
    //     duplicates, so the dedup problem does not arise rather than being
    //     worked around.
    // `closed_at` carries a minmax index, so resolving the bound is cheap; the
    // LP chart resolves its window the same way. `FINAL` on the appearances
    // matches the detail stat so re-ingest duplicates collapse identically.
    let ids = list_rows
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    // List recent_invocations only; recent_events is detail-only (the list DTO
    // has no events field), so this path keeps the single windowed count.
    let days = STATS_WINDOW_DAYS;
    let count_sql = format!(
        "SELECT \
            sia.contract_id                  AS contract_id, \
            toUInt64(count())                AS recent_invocations \
         FROM soroban_invocations_appearances sia FINAL \
         WHERE sia.contract_id IN ({ids}) \
           AND sia.ledger_sequence >= ( \
               SELECT min(sequence) FROM ledgers \
               WHERE closed_at >= now64() - INTERVAL {days} DAY \
           ) \
         GROUP BY sia.contract_id"
    );
    // Resolve the page's deployer surrogates → StrKeys by a bloom-pruned
    // key-seek (`accounts.idx_acc_id`), replacing the full-table `accounts`
    // join (task 0319). `deployer_id = 0` means no deployer → skip.
    let deployer_ids: Vec<i64> = list_rows
        .iter()
        .filter_map(|r| r.deployer_id)
        .filter(|&d| d != 0)
        .collect();

    // Step 4 (task 0441): resolve the page's SAC contracts → mirrored assets.
    // Skipped entirely on a SAC-free page (the common case — SACs are ~2.9%
    // of contracts and the default newest-50 page holds none).
    let sac_ids: Vec<i64> = list_rows
        .iter()
        .filter(|r| r.is_sac)
        .map(|r| r.id)
        .collect();

    // All three read only `list_rows` — none consumes another's output, so they
    // go out together instead of as three serial round trips (task 0446).
    let (count_rows, deployers, sac_assets) = tokio::join!(
        client.query(&count_sql).fetch_all::<InvocationCountChRow>(),
        resolve_accounts(client, deployer_ids),
        fetch_sac_assets(client, &sac_ids),
    );
    let counts: HashMap<i64, i64> = count_rows?
        .into_iter()
        .map(|r| (r.contract_id, r.recent_invocations as i64))
        .collect();
    let deployers = deployers?;
    let sac_assets = sac_assets?;

    Ok(list_rows
        .into_iter()
        .map(|r| ContractListRow {
            recent_invocations: counts.get(&r.id).copied().unwrap_or(0),
            contract_type_name: r.contract_type.and_then(contract_type_name),
            deployer: r
                .deployer_id
                .and_then(|d| deployers.get(&d))
                .filter(|s| !s.is_empty())
                .cloned(),
            sac_asset: sac_assets.get(&r.id).cloned(),
            id: r.id,
            contract_id: r.contract_id,
            contract_type: r.contract_type,
            is_sac: r.is_sac,
            deployed_at_ledger: r.deployed_at_ledger,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Detail header — canonical 11 Statement A
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ContractHeaderChRow {
    id: i64,
    contract_id: String,
    wasm_hash: Option<String>,
    wasm_uploaded_at_ledger: Option<i64>,
    deployer_id: Option<i64>,
    deployed_at_ledger: Option<i64>,
    contract_type: Option<i16>,
    is_sac: bool,
    // Task 0327 — mutability as a tri-state Int8 from the joined WASM metadata:
    // 1 = self-upgradeable, 0 = frozen, -1 = Unknown (no key / no row). See the
    // SQL expression in `fetch_contract` and `map_upgradeable`.
    upgradeable: i8,
}

pub async fn fetch_contract(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<ContractRow>, clickhouse::error::Error> {
    // Two FINAL/aliasing pitfalls, both 500'd every contract detail (regression
    // from task 0327):
    //   1. `wasm_interface_metadata` is a plain `MergeTree`, so it must NOT carry
    //      `FINAL` — CH rejects `FINAL` on a non-replacing engine with
    //      `Code: 181 (ILLEGAL_FINAL)`. Only `soroban_contracts` (Replacing)
    //      takes `FINAL`. (The events stats query below already joins `wim`
    //      FINAL-free.)
    //   2. `sc.id` MUST be aliased `AS id`: `id` is ambiguous across the joined
    //      tables (`soroban_contracts`, `accounts`), so CH names the result
    //      column `sc.id`, which the `clickhouse` row deserialiser can't match
    //      to the `ContractHeaderChRow.id` field → "schema mismatch".
    let row = client
        .query(
            "SELECT \
                sc.id                                  AS id, \
                sc.contract_id, \
                lower(hex(sc.wasm_hash))               AS wasm_hash, \
                nullIf(sc.wasm_uploaded_at_ledger, 0)  AS wasm_uploaded_at_ledger, \
                sc.deployer_id                         AS deployer_id, \
                sc.deployed_at_ledger                  AS deployed_at_ledger, \
                sc.contract_type                       AS contract_type, \
                sc.is_sac                              AS is_sac, \
                toInt8(if(JSONHas(wim.metadata, 'upgradeable'), \
                          JSONExtractBool(wim.metadata, 'upgradeable'), -1)) AS upgradeable \
             FROM soroban_contracts sc FINAL \
             LEFT JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash \
             WHERE sc.contract_id = ? \
             LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<ContractHeaderChRow>()
        .await?;

    let Some(r) = row else { return Ok(None) };
    // Resolve the deployer StrKey by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN … ON deployer.id = sc.deployer_id` (task 0345).
    // Task 0441 — the mirrored asset; `fetch_sac_assets` returns empty for a
    // non-SAC, so the pair goes out together rather than one after the other
    // (task 0446).
    let sac_id = [r.id];
    let sac_ids: &[i64] = if r.is_sac { &sac_id } else { &[] };
    let (accounts, sac_assets) = tokio::join!(
        resolve_accounts(client, r.deployer_id.into_iter().collect()),
        fetch_sac_assets(client, sac_ids),
    );
    let accounts = accounts?;
    let sac_asset = sac_assets?.remove(&r.id);
    Ok(Some(ContractRow {
        sac_asset,
        upgradeable: map_upgradeable(r.wasm_hash.is_some(), r.upgradeable),
        id: r.id,
        contract_id: r.contract_id,
        wasm_hash: r.wasm_hash,
        wasm_uploaded_at_ledger: r.wasm_uploaded_at_ledger,
        deployer: r
            .deployer_id
            .and_then(|id| accounts.get(&id).cloned())
            .filter(|s| !s.is_empty()),
        deployed_at_ledger: r.deployed_at_ledger,
        contract_type_name: r.contract_type.and_then(contract_type_name),
        contract_type: r.contract_type,
        is_sac: r.is_sac,
    }))
}

/// Task 0327 — map the tri-state Int8 from `fetch_contract` into `upgradeable`:
/// - no WASM (SAC / `wasm_hash IS NULL`) → `Some(false)` (cannot self-upgrade),
///   regardless of the join (SAC has no metadata row).
/// - `1` → `Some(true)` (self-upgradeable), `0` → `Some(false)` (frozen).
/// - `-1` → `None` (Unknown: no metadata row, or a row predating task 0327 →
///   the frontend renders no chip).
fn map_upgradeable(has_wasm: bool, code: i8) -> Option<bool> {
    if !has_wasm {
        return Some(false);
    }
    match code {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Bounded-window stats — canonical 11 Statement B
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct StatsChRow {
    recent_invocations: u64,
    recent_unique_callers: u64,
    recent_events: u64,
}

/// `window` is the echoed label (e.g. `"7 days"`); its leading integer is the
/// day count. CH `soroban_invocations_appearances` has no `created_at`, so the
/// window is applied via a JOIN to `ledgers.closed_at`, bounded first by a
/// `ledger_sequence` floor so the seek stays on the primary-key prefix.
///
/// `recent_events` is computed as a scalar subquery over `soroban_events` in the
/// SAME window. Parity with PG is by construction, NOT by event-type filtering:
/// both `soroban_events` (CH) and `soroban_events_appearances` (PG) are written
/// from the one parser `ExtractedEvent` stream, which drops `diagnostic_events`
/// at parse time (`xdr-parser::types`; ADR 0033) but keeps System + Contract
/// events. CH unfolds one row per event (no appearance-fold `amount` on CH), so a
/// plain `count()` equals PG's `SUM(amount)` over the same population — measured
/// global mix is ~9.25B Contract (`event_type = 1`) + ~4.7K System (`= 0`), zero
/// Diagnostic. Do NOT add an `event_type = 1` filter here: PG counts System too,
/// so filtering would break parity rather than tighten it.
///
/// `count()` (not `uniqExact` over the event key) is load-bearing: the hottest
/// contract has ~76M events in the 7-day window, and `uniqExact` builds a hash
/// set of every key → blows the `api_reader` per-query cap (Code 241, measured
/// OOM at 3.73 GiB). `count()` streams: measured on that contract it reads
/// ~99.5M rows / 1.39 GiB in 0.24 s at 89 MiB peak — far under the `read_only`
/// 30 s / 4 GB per-query cap, and the 45 s detail-response cache bounds the
/// `api_throttle` 50 B-rows/h aggregate draw. We also skip `FINAL` — re-ingest
/// duplicates are absent in practice (`count()` and `count() FINAL` agree, and
/// `count() == uniqExact` on every sampled contract from 1.2M to 4.8M events)
/// and `FINAL`-on-`soroban_events` is the documented OOM path ([`fetch_events`]).
//
// ponytail: `now64()` window zeros out the whole stats trio when ingest lag
// exceeds the window (inherited from recent_invocations); a staleness-aware
// window is a separate concern, not fixed here.
// ponytail: plain count() can over-count a re-ingested ledger range; swap to a
// deduped subquery (or FINAL) only if a re-ingest ever skews the 7-day figure.
pub async fn fetch_contract_stats(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
) -> Result<ContractStats, clickhouse::error::Error> {
    let sql = contract_stats_sql(STATS_WINDOW_DAYS);
    let row = client
        .query(&sql)
        .bind(contract_surrogate_id)
        .bind(contract_surrogate_id)
        .fetch_one::<StatsChRow>()
        .await?;

    Ok(ContractStats {
        recent_invocations: row.recent_invocations as i64,
        recent_unique_callers: row.recent_unique_callers as i64,
        recent_events: row.recent_events as i64,
        stats_window: stats_window_label(),
    })
}

/// SQL for [`fetch_contract_stats`]. `days` derives from the
/// operator-controlled window label (not user input) — safe to interpolate.
/// Two `?` placeholders bind `contract_surrogate_id` (events subquery, then the
/// outer invocations seek), in source order.
///
/// The `recent_events` scalar subquery is wrapped in `ifNull(…, 0)`:
/// ClickHouse types a `(SELECT …)` scalar as `Nullable(UInt64)`, which
/// mismatches the non-nullable `u64` in `StatsChRow` and fails RowBinary decode
/// → `db_error` 500 on every contract detail. `count()` never actually yields
/// NULL, so the `0` branch is unreachable; it only fixes the static type.
///
/// Both windows are ONE `ledger_sequence >= (first sequence in the window)`
/// bound, resolved from the data (lore-0420). `ledgers` is a ReplacingMergeTree
/// with unmerged duplicate rows, so JOINing it multiplied every appearance/event
/// row per duplicate copy and inflated `count()`. `min()` is immune to
/// duplicates, so this shape removes the fan-out instead of compensating for it —
/// and drops the hardcoded ledgers-per-day floor at the same time.
/// `uniqExact(caller_id)` was already fan-out-safe (a distinct set); only the raw
/// `count()`s were affected.
fn contract_stats_sql(days: i64) -> String {
    format!(
        "SELECT \
            toUInt64(count())                       AS recent_invocations, \
            toUInt64(uniqExact(sia.caller_id))      AS recent_unique_callers, \
            ifNull(( \
                SELECT toUInt64(count()) \
                FROM soroban_events se \
                WHERE se.contract_id = ? \
                  AND se.ledger_sequence >= ( \
                      SELECT min(sequence) FROM ledgers \
                      WHERE closed_at >= now64() - INTERVAL {days} DAY \
                  ) \
            ), 0)                                   AS recent_events \
         FROM soroban_invocations_appearances sia FINAL \
         WHERE sia.contract_id = ? \
           AND sia.ledger_sequence >= ( \
               SELECT min(sequence) FROM ledgers \
               WHERE closed_at >= now64() - INTERVAL {days} DAY \
           )"
    )
}

#[cfg(test)]
mod stats_sql_tests {
    use super::{STATS_WINDOW_DAYS, contract_stats_sql, stats_window_label};

    /// The wire label must stay derived from the constant, so the number the
    /// SQL windows on and the string the client is told can never disagree.
    #[test]
    fn wire_label_is_derived_from_the_window_constant() {
        assert_eq!(stats_window_label(), format!("{STATS_WINDOW_DAYS} days"));
        assert!(stats_window_label().starts_with(&STATS_WINDOW_DAYS.to_string()));
    }

    // Regression guard for task 0300: CH `recent_events` was hardcoded `0`.
    // The stats SQL MUST select a real windowed event count off `soroban_events`
    // (parity with PG's appearance-fold SUM), not a literal.
    #[test]
    fn stats_sql_computes_recent_events_from_events_table() {
        let sql = contract_stats_sql(7);

        assert!(
            sql.contains("AS recent_events"),
            "recent_events column missing: {sql}"
        );
        assert!(
            sql.contains("FROM soroban_events se"),
            "recent_events must read soroban_events: {sql}"
        );
        // The bug shape: a bare literal aliased to recent_events. Collapse
        // whitespace first so the guard is alignment-independent.
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            !normalized.contains("0 AS recent_events"),
            "recent_events still hardcoded to a literal: {sql}"
        );
        // Window parity: both the invocations seek and the events subquery
        // apply the same ledger floor + INTERVAL N DAY bound.
        assert_eq!(
            sql.matches("INTERVAL 7 DAY").count(),
            2,
            "events window must mirror the invocations window: {sql}"
        );
        // Two binds: events-subquery contract_id, then outer contract_id.
        assert_eq!(
            sql.matches("contract_id = ?").count(),
            2,
            "expected two `contract_id = ?` binds: {sql}"
        );
        // The scalar subquery MUST be `ifNull(…, 0)`-wrapped: CH types a bare
        // `(SELECT …)` as Nullable(UInt64), which fails the non-nullable `u64`
        // decode → 500 on every contract detail.
        assert!(
            normalized.contains("ifNull(( SELECT toUInt64(count())"),
            "recent_events subquery must be ifNull-wrapped: {sql}"
        );
    }

    /// Regression guard for lore-0420. Two failure modes, one shape.
    ///
    /// `ledgers` is a ReplacingMergeTree with unmerged duplicate rows, so
    /// JOINing it into a `count()` multiplies the count by the number of
    /// physical copies (measured ~1.6x). And the seek bound must be resolved
    /// from the data, not from a hardcoded ledgers-per-day constant: the old
    /// `days * 17_280` assumed a 5 s cadence, ran 13% wide against the real
    /// ~5.6 s, and would silently run SHORT — under-reporting the window — if
    /// the chain ever sped up.
    ///
    /// One `min(sequence)` bound satisfies both: immune to duplicates, exact by
    /// construction.
    #[test]
    fn stats_sql_bounds_window_from_data_never_a_join_or_a_constant() {
        let sql = contract_stats_sql(7);
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");

        assert!(
            !normalized.contains("JOIN ledgers"),
            "a JOIN onto ledgers fans each row out per duplicate copy and \
             inflates the count: {sql}"
        );
        assert!(
            !normalized.contains("17280") && !normalized.contains("17_280"),
            "the window bound must come from the data, not a ledgers-per-day \
             constant that drifts with the chain cadence: {sql}"
        );
        // One per window: the invocations seek and the events subquery.
        assert_eq!(
            normalized
                .matches("SELECT min(sequence) FROM ledgers WHERE closed_at >=")
                .count(),
            2,
            "both the invocations and events windows must derive their bound \
             from the data: {sql}"
        );
    }
}

// ---------------------------------------------------------------------------
// Interface — canonical 12
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct InterfaceChRow {
    contract_id: String,
    wasm_hash: Option<String>,
    /// Raw JSON text from `wasm_interface_metadata.metadata` (empty when the
    /// contract has no WASM metadata row).
    metadata: String,
}

/// `Ok(None)` only when the contract row itself is missing; SAC / pre-upload /
/// stub contracts return `Ok(Some(_))` with `interface_metadata = None`. The
/// "has a `functions` key" stub filter (canonical 12 / PG `metadata ?
/// 'functions'`) is applied in Rust on the decoded JSON.
pub async fn fetch_wasm_interface(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<InterfaceRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                sc.contract_id, \
                lower(hex(sc.wasm_hash))        AS wasm_hash, \
                ifNull(wim.metadata, '')        AS metadata \
             FROM soroban_contracts sc FINAL \
             LEFT JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash \
             WHERE sc.contract_id = ? \
             LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<InterfaceChRow>()
        .await?;

    Ok(row.map(|r| {
        let interface_metadata = serde_json::from_str::<serde_json::Value>(&r.metadata)
            .ok()
            .filter(|v| v.get("functions").is_some());
        InterfaceRow {
            contract_id: r.contract_id,
            wasm_hash: r.wasm_hash,
            interface_metadata,
        }
    }))
}

// ---------------------------------------------------------------------------
// Invocations — canonical 13 (two-step, multi-partition-safe)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct InvocationKeyRow {
    ledger_sequence: i64,
    transaction_id: i64,
    caller_id: Option<i64>,
}

#[derive(Debug, Row, Deserialize)]
struct TxMetaChRow {
    transaction_id: i64,
    hash: String,
    successful: bool,
    created_at: i64,
}

/// `contract_surrogate_id` is from [`fetch_contract`]; caller passes
/// `limit + 1`. Driven off `soroban_invocations_appearances` (leading-PK seek
/// on `contract_id`), then the page's transaction header columns
/// (`hash` / `successful` / `closed_at`) are fetched by
/// `(ledger_sequence, id) IN (keys)` and merged. The CH cursor keys on
/// `(ledger_sequence, transaction_id)`.
pub async fn fetch_invocation_appearances(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    let (cursor_ledger, cursor_tiebreak): (Option<i64>, Option<i64>) = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => (Some(*ledger_sequence), Some(*tiebreak)),
        _ => (None, None),
    };
    let (op, order) = keyset_sql_desc(direction);

    // Inline the cursor bounds rather than `.bind()`-ing them: the clickhouse
    // 0.15 bound-parameter path returns an empty result when `None` is bound
    // into a tuple keyset comparison (the same defect that forced transactions
    // B/C to inline). Values are i64 / None→NULL, no injection surface.
    let cl = cursor_ledger.map_or_else(|| "NULL".to_string(), |v| v.to_string());
    let ct = cursor_tiebreak.map_or_else(|| "NULL".to_string(), |v| v.to_string());

    // Step 1: contract-scoped driver seek. `contract_id` is the leading PK of
    // `soroban_invocations_appearances`, so the inner subquery reads only this
    // contract's rows.
    //
    // The page LIMIT is applied INSIDE the subquery, BEFORE the `accounts caller`
    // join. That join has no FINAL (a 16M-row accounts FINAL would be ruinous),
    // and a hot contract has millions of invocations; joining accounts to ALL of
    // them before the limit OOMs the JoiningTransform (measured: 14.9M
    // invocations → 300M join rows → 5.6 GiB limit hit). With the limit inside,
    // the join sees only ≤limit rows. FINAL is dropped on the seek too — with it
    // CH merges the contract's rows across every part (~38× read amplification,
    // measured 574M vs 18.6M rows); the outer `LIMIT 1 BY (ledger_sequence,
    // transaction_id)` collapses both the caller-account fan-out and any rare
    // re-ingest duplicate, so FINAL is not needed for correctness here.
    let driver_sql = format!(
        "SELECT \
            m.ledger_sequence AS ledger_sequence, \
            m.transaction_id AS transaction_id, \
            m.caller_id AS caller_id \
         FROM ( \
            SELECT ledger_sequence, transaction_id, caller_id \
            FROM soroban_invocations_appearances \
            WHERE contract_id = ? \
              AND ledger_sequence <= (SELECT max(sequence) FROM ledgers) \
              AND ({cl} IS NULL OR (ledger_sequence, transaction_id) {op} ({cl}, {ct})) \
            ORDER BY ledger_sequence {order}, transaction_id {order} \
            LIMIT ? \
         ) m \
         LIMIT 1 BY m.ledger_sequence, m.transaction_id"
    );
    let key_rows = client
        .query(&driver_sql)
        .bind(contract_surrogate_id)
        .bind(limit)
        .fetch_all::<InvocationKeyRow>()
        .await?;

    if key_rows.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<(i64, i64)> = key_rows
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

    // Step 2: fetch the transaction header columns for the page's keys.
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
            t.id AS transaction_id, \
            lower(hex(t.hash)) AS hash, \
            t.successful, \
            l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions})"
    );
    // Caller StrKeys resolve by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN … ON caller.id = m.caller_id` (task 0345).
    // Both reads key off `key_rows` alone, so they go out together (task 0446).
    let (tx_rows, accounts) = tokio::join!(
        client.query(&page_sql).fetch_all::<TxMetaChRow>(),
        resolve_accounts(
            client,
            key_rows.iter().filter_map(|r| r.caller_id).collect()
        ),
    );
    let tx_rows = tx_rows?;
    let accounts = accounts?;

    let mut tx_by_id: HashMap<i64, TxMetaChRow> = HashMap::with_capacity(tx_rows.len());
    for row in tx_rows {
        tx_by_id.insert(row.transaction_id, row);
    }

    // Emit in driver keyset order, merging the transaction header columns. A
    // key whose transaction row is somehow absent is skipped (should not occur
    // — an invocation appearance always has its parent transaction).
    let mut out = Vec::with_capacity(key_rows.len());
    for key in &key_rows {
        let Some(tx) = tx_by_id.get(&key.transaction_id) else {
            continue;
        };
        out.push(InvocationAppearanceRow {
            transaction_id: key.transaction_id,
            transaction_hash: tx.hash.clone(),
            ledger_sequence: key.ledger_sequence,
            created_at: millis_to_utc(tx.created_at),
            caller_account: key
                .caller_id
                .and_then(|id| accounts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            successful: tx.successful,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Events — GET /v1/contracts/:id/events (canonical 14, full-content CH read)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct EventChRow {
    ledger_sequence: i64,
    transaction_id: i64,
    event_index: i16,
    event_type: i16,
    /// ScVal already JSON-decoded at ingest (column name is a misnomer).
    topics_xdr: String,
    data_xdr: String,
    transaction_hash: String,
    successful: bool,
    /// `ledgers.closed_at` millis.
    created_at: i64,
}

/// Step-1 page row: the event payload off `soroban_events` alone (no joins).
/// `transaction_hash` / `successful` / `created_at` are resolved in step 2 (task
/// 0317).
#[derive(Debug, Row, Deserialize)]
struct EventPageRow {
    ledger_sequence: i64,
    transaction_id: i64,
    event_index: i16,
    event_type: i16,
    topics_xdr: String,
    data_xdr: String,
}

/// Step-2 resolve row: `transactions.id` → `(hash, successful)`.
#[derive(Debug, Row, Deserialize)]
struct EventTxRow {
    id: i64,
    /// Already `lower(hex())` in the query.
    hash: String,
    successful: bool,
}

/// Step-2 resolve row: `ledgers.sequence` → `closed_at` (millis).
#[derive(Debug, Row, Deserialize)]
struct EventLedgerRow {
    sequence: i64,
    closed_at: i64,
}

/// A decoded event row + its `event_index` (the cursor tie-break, which is not
/// carried on the `EventItem` wire). The handler finalises the page over these,
/// builds the `EventCursor::Ch` from the boundary row, then maps to `EventItem`.
pub struct ChEvent {
    pub event_index: i16,
    pub item: EventItem,
}

fn map_event_row(r: EventChRow) -> ChEvent {
    // `topics` is a JSON array of ScVals; mirror the PG `expand_events` shaping
    // (array → its elements, scalar → singleton). A decode failure degrades to
    // empty/null rather than dropping the row.
    let topics = serde_json::from_str::<serde_json::Value>(&r.topics_xdr)
        .map(|v| match v {
            serde_json::Value::Array(a) => a,
            other => vec![other],
        })
        .unwrap_or_default();
    let data =
        serde_json::from_str::<serde_json::Value>(&r.data_xdr).unwrap_or(serde_json::Value::Null);
    let event_type = ContractEventType::try_from(r.event_type)
        .map(|e| e.to_string())
        .unwrap_or_default();
    ChEvent {
        event_index: r.event_index,
        item: EventItem {
            transaction_hash: r.transaction_hash,
            ledger_sequence: r.ledger_sequence,
            transaction_id: r.transaction_id,
            successful: r.successful,
            created_at: millis_to_utc(r.created_at),
            event_type,
            topics,
            data,
        },
    }
}

/// `contract_surrogate_id` is from [`fetch_contract`]; caller passes the
/// handler's `fetch_limit()` (already the peek `+1`). Single-statement
/// contract-leading PK seek on `soroban_events` (no two-step driver needed —
/// the payload is inline, unlike invocations/account-tx which fan out to
/// `transactions`). `FINAL` on the per-contract seek collapses re-ingest
/// duplicates; the `transactions` / `ledgers` joins carry NO `FINAL` (a tx is
/// immutable, so a dup version is identical) and `LIMIT 1 BY` the event key
/// collapses any join fan-out. CH cursor keys on
/// `(ledger_sequence, transaction_id, event_index)`; a `Pg`-variant cursor never
/// reaches here (the handler's cross-source guard rejects it).
pub async fn fetch_events(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&EventCursor>,
    direction: Direction,
) -> Result<Vec<ChEvent>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Inline the cursor bounds (i64 / i16 — no injection surface); the clause is
    // omitted entirely on the first page so no NULL is bound into the tuple
    // keyset (the clickhouse 0.15 None-in-tuple defect).
    let cursor_clause = match cursor {
        Some(EventCursor::Ch {
            ledger_sequence,
            transaction_id,
            event_index,
        }) => format!(
            " AND (se.ledger_sequence, se.transaction_id, se.event_index) {op} \
             ({ledger_sequence}, {transaction_id}, {event_index})"
        ),
        _ => String::new(),
    };

    // Step 1: page the events via the `contract_id` PK seek — NO joins (task
    // 0317). The previous form `JOIN transactions t` / `INNER JOIN ledgers l`
    // made ClickHouse build the join hash side from the WHOLE `transactions`
    // table (billions of rows) → `MEMORY_LIMIT_EXCEEDED` (Code 241).
    //
    // `FINAL` is also DROPPED here — and that is load-bearing, not cosmetic. On a
    // hot contract (millions of events across many parts) `FINAL` merges the
    // whole per-contract range, reading the heavy `topics_xdr`/`data_xdr`
    // columns, and OOMs (Code 241) under the prod `api_reader` 4 GB cap
    // (reproduced: FINAL OOMs at 500 MB–2 GB, only barely survives 4 GB). The
    // full-key `LIMIT 1 BY (ledger_sequence, transaction_id, event_index)`
    // already collapses re-ingest duplicates, and every projected column is
    // immutable across ReplacingMergeTree versions, so a non-FINAL read returns
    // identical rows — the read-in-order page then short-circuits at `LIMIT`
    // instead of merging the whole contract. Same rationale as transactions
    // Statement A (task 0290).
    let page_sql = format!(
        "SELECT \
            se.ledger_sequence              AS ledger_sequence, \
            se.transaction_id               AS transaction_id, \
            se.event_index                  AS event_index, \
            se.event_type                   AS event_type, \
            se.topics_xdr                   AS topics_xdr, \
            se.data_xdr                     AS data_xdr \
         FROM soroban_events se \
         WHERE se.contract_id = ? AND se.ledger_sequence <= (SELECT max(sequence) FROM ledgers){cursor_clause} \
         ORDER BY se.ledger_sequence {order}, se.transaction_id {order}, se.event_index {order} \
         LIMIT 1 BY se.ledger_sequence, se.transaction_id, se.event_index \
         LIMIT ?"
    );

    let raw = client
        .query(&page_sql)
        .bind(contract_surrogate_id)
        .bind(limit)
        .fetch_all::<EventPageRow>()
        .await?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: resolve the page's `transaction_hash` / `successful` / `closed_at`
    // with PK-prefix key-seeks instead of full-table hash joins (mirrors
    // `transactions::queries::resolve_source_and_closed_at`, task 0290).
    // `transactions WHERE ledger_sequence IN (...)` prunes by the PK prefix to
    // the handful of ledgers on this page, then filters `id IN (...)`; no
    // `FINAL` (a transaction is immutable, so a dup version is identical).
    // `ledgers WHERE sequence IN (...)` is a plain PK seek.
    let in_list = |vals: &[i64]| {
        vals.iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let dedup = |f: fn(&EventPageRow) -> i64| -> Vec<i64> {
        let mut v: Vec<i64> = raw.iter().map(f).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let ledger_seqs = dedup(|r| r.ledger_sequence);
    let tx_ids = dedup(|r| r.transaction_id);

    // Both seeks key off `raw` alone, so they go out together (task 0446).
    let (tx_rows, ledger_rows) = tokio::join!(
        client
            .query(&format!(
                "SELECT id AS id, lower(hex(hash)) AS hash, successful AS successful \
                 FROM transactions WHERE ledger_sequence IN ({}) AND id IN ({}) LIMIT 1 BY id",
                in_list(&ledger_seqs),
                in_list(&tx_ids),
            ))
            .fetch_all::<EventTxRow>(),
        client
            .query(&format!(
                "SELECT sequence AS sequence, closed_at AS closed_at FROM ledgers WHERE sequence IN ({})",
                in_list(&ledger_seqs),
            ))
            .fetch_all::<EventLedgerRow>(),
    );

    let txs: std::collections::HashMap<i64, (String, bool)> = tx_rows?
        .into_iter()
        .map(|r| (r.id, (r.hash, r.successful)))
        .collect();

    let closed_ats: std::collections::HashMap<i64, i64> = ledger_rows?
        .into_iter()
        .map(|r| (r.sequence, r.closed_at))
        .collect();

    // Rebuild full event rows in page order, then map. A missing tx/ledger
    // lookup defaults rather than drops the row, so the page count (and the
    // peek `+1` next-page detection) is preserved.
    Ok(raw
        .into_iter()
        .map(|r| {
            let (transaction_hash, successful) =
                txs.get(&r.transaction_id).cloned().unwrap_or_default();
            let created_at = closed_ats.get(&r.ledger_sequence).copied().unwrap_or(0);
            map_event_row(EventChRow {
                ledger_sequence: r.ledger_sequence,
                transaction_id: r.transaction_id,
                event_index: r.event_index,
                event_type: r.event_type,
                topics_xdr: r.topics_xdr,
                data_xdr: r.data_xdr,
                transaction_hash,
                successful,
                created_at,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_type_name_matches_pg_function() {
        assert_eq!(contract_type_name(0).as_deref(), Some("token"));
        assert_eq!(contract_type_name(1).as_deref(), Some("other"));
        assert_eq!(contract_type_name(2).as_deref(), Some("nft"));
        assert_eq!(contract_type_name(3).as_deref(), Some("fungible"));
        assert_eq!(contract_type_name(4), None);
    }

    #[test]
    fn map_upgradeable_three_state() {
        // SAC / no WASM → Immutable regardless of the join code.
        assert_eq!(map_upgradeable(false, -1), Some(false));
        assert_eq!(map_upgradeable(false, 1), Some(false));
        // WASM present: 1 → upgradeable, 0 → frozen.
        assert_eq!(map_upgradeable(true, 1), Some(true));
        assert_eq!(map_upgradeable(true, 0), Some(false));
        // WASM present, -1 (no metadata row / pre-0327 key absent) → Unknown.
        assert_eq!(map_upgradeable(true, -1), None);
    }

    fn event_row(event_type: i16, topics_xdr: &str, data_xdr: &str) -> EventChRow {
        EventChRow {
            ledger_sequence: 100,
            transaction_id: 7,
            event_index: 2,
            event_type,
            topics_xdr: topics_xdr.to_string(),
            data_xdr: data_xdr.to_string(),
            transaction_hash: "deadbeef".to_string(),
            successful: true,
            created_at: 1_700_000_000_000,
        }
    }

    #[test]
    fn map_event_row_decodes_payload_type_and_index() {
        let ev = map_event_row(event_row(
            1, // contract
            r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GABC"}]"#,
            r#"{"type":"i128","value":"1000"}"#,
        ));
        assert_eq!(ev.event_index, 2); // cursor tie-break preserved off the wire
        assert_eq!(ev.item.event_type, "contract");
        assert_eq!(ev.item.topics.len(), 2); // JSON array → its elements
        assert_eq!(ev.item.data["value"], "1000");
        assert_eq!(ev.item.transaction_hash, "deadbeef");
        assert_eq!(ev.item.ledger_sequence, 100);
        assert!(ev.item.successful);
    }

    #[test]
    fn map_event_row_scalar_topics_wraps_singleton() {
        let ev = map_event_row(event_row(0 /* system */, r#""solo""#, "null"));
        assert_eq!(ev.item.event_type, "system");
        assert_eq!(ev.item.topics.len(), 1); // scalar JSON → singleton vec
        assert!(ev.item.data.is_null());
    }

    #[test]
    fn map_event_row_event_type_labels_and_out_of_range() {
        assert_eq!(
            map_event_row(event_row(0, "[]", "null")).item.event_type,
            "system"
        );
        assert_eq!(
            map_event_row(event_row(1, "[]", "null")).item.event_type,
            "contract"
        );
        assert_eq!(
            map_event_row(event_row(2, "[]", "null")).item.event_type,
            "diagnostic"
        );
        // Out-of-range discriminant → empty string (try_from fails, default).
        assert_eq!(
            map_event_row(event_row(99, "[]", "null")).item.event_type,
            ""
        );
    }

    #[test]
    fn map_event_row_malformed_payload_degrades_not_drops() {
        let ev = map_event_row(event_row(1, "not json", "also not json"));
        assert!(ev.item.topics.is_empty()); // decode fail → empty, row still emitted
        assert!(ev.item.data.is_null());
    }
}
