//! ClickHouse queries for the assets endpoints (task 0243).
//!
//! Mirrors the PG path (`queries.rs`) one-for-one — same `AssetRow` shape, so
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
//!   keyed on the 4-tuple). The `accounts` / `soroban_contracts` lookup joins
//!   carry no FINAL — same convention as the contracts/accounts CH detail
//!   reads: the identity columns they project (issuer StrKey, contract StrKey,
//!   write-once `home_domain` / `deployed_at_ledger`) are stable across
//!   versions, and a 16M-row `accounts FINAL` would be ruinous.
//! - **`nullIf(...)` on the joined columns** (issuer / contract_id /
//!   home_domain → `nullIf(_, '')`, deployed_at_ledger → `nullIf(_, 0)`) makes a
//!   LEFT-JOIN miss decode as `None`. We do NOT use `SETTINGS join_use_nulls = 1`
//!   — the `api_reader` CH user runs under `readonly = 1` (RBAC profile
//!   `read_only`), which rejects per-query setting overrides. Same convention as
//!   stkrolikiewicz's CH modules (accounts `fetch_balances`, LP).
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

use crate::common::ch::{self, millis_to_utc};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

use super::queries::{AssetRow, AssetTxRow, ResolvedListParams};

/// `asset_type` SMALLINT → canonical label, matching the PG
/// `token_asset_type_name` function. `None` for an out-of-range code (the PG
/// `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native"),
        1 => Some("classic_credit"),
        2 => Some("sac"),
        3 => Some("soroban"),
        _ => None,
    }
    .map(str::to_string)
}

/// Shared projection — column order MUST match [`AssetChRow`] (positional decode).
/// `nullIf(asset_code, '')` collapses the native sentinel to `None`; the joined
/// `issuer` / `contract_id` / `home_domain` / `deployed_at_ledger` decode to
/// `None` on a JOIN miss via the `nullIf(...)` wraps (readonly-safe; no
/// `SETTINGS join_use_nulls`).
// Enrichment (icon_url + classic/SAC name) is read from the `asset_enrichment`
// side table (ADR 0050 / task 0231), NOT the indexer-owned `assets.{icon_url,
// name}` placeholders (dropped, task 0231 step 8). Per Option C the name has a
// single owner per `asset_type`, composed disjointly at read:
//   classic/SAC (1,2) → `asset_enrichment.name`
//   soroban (3)       → `soroban_contract_metadata.name` (on-chain instance
//                       `METADATA` struct; the legacy `soroban_contracts.name`
//                       column is dead — no writer since task 0297)
//   native (0)        → the `"Stellar Lumen"` literal
// `asset_enrichment` is `ReplacingMergeTree(version)`; the `argMax(_, version)`
// sub-aggregate collapses it to one latest row per key so the LEFT JOIN can't
// multiply asset rows on un-merged duplicates. `''` is the sentinel
// ("tried, nothing"), neutralised with `nullIf`.
//
// On-chain token metadata (`soroban_contract_metadata` m, task 0297) is joined
// via `sc.contract_id` — `assets` carries only the surrogate `a.contract_id`,
// not the strkey, so the metadata key is resolved through `soroban_contracts`.
// Consequence: a soroban asset whose `soroban_contracts` row is missing/stale
// joins NULL metadata even when the side table holds it. Acceptable — the
// deploy is always indexed before transfers create the asset row — but it is a
// structural coupling, not an independent join. The subquery reads the metadata
// table with `FINAL` (latest whole row per contract_id) — rows are whole-struct
// snapshots at one ledger, so `FINAL` is the direct, frankenstein-proof way to
// collapse the RMT vs a per-column `argMax`. Table is bounded (Soroban-native
// tokens only, SACs skipped) so the read-time merge is cheap.
const ASSET_CH_SELECT: &str = "SELECT \
     a.asset_type                 AS asset_type, \
     nullIf(a.asset_code, '')     AS asset_code, \
     nullIf(iss.account_id, '')   AS issuer, \
     nullIf(iss.home_domain, '')  AS issuer_home_domain, \
     nullIf(sc.contract_id, '')   AS contract_id, \
     coalesce(nullIf(ae.name, ''), nullIf(m.name, ''), \
              if(a.asset_type = 0, 'Stellar Lumen', NULL)) AS name, \
     nullIf(m.symbol, '')         AS symbol, \
     coalesce(m.decimals, 7)      AS decimals, \
     toString(a.total_supply)     AS total_supply, \
     a.holder_count               AS holder_count, \
     nullIf(sc.deployed_at_ledger, 0) AS deployed_at_ledger, \
     nullIf(ae.icon_url, '')      AS icon_url, \
     a.issuer_id                  AS issuer_id_key, \
     a.contract_id                AS contract_id_key \
     FROM assets a FINAL \
     LEFT JOIN accounts iss          ON iss.id = a.issuer_id \
     LEFT JOIN soroban_contracts sc  ON sc.id  = a.contract_id \
     LEFT JOIN ( \
         SELECT contract_id, name, symbol, decimals \
         FROM soroban_contract_metadata FINAL \
     ) m ON m.contract_id = sc.contract_id \
     LEFT JOIN ( \
         SELECT asset_type, asset_code, issuer_id, contract_id, \
                argMax(icon_url, version) AS icon_url, \
                argMax(name, version)     AS name \
         FROM asset_enrichment \
         GROUP BY asset_type, asset_code, issuer_id, contract_id \
     ) ae ON ae.asset_type  = a.asset_type  AND ae.asset_code  = a.asset_code \
         AND ae.issuer_id   = a.issuer_id   AND ae.contract_id = a.contract_id";

#[derive(Debug, Row, Deserialize)]
struct AssetChRow {
    asset_type: i16,
    asset_code: Option<String>,
    issuer: Option<String>,
    issuer_home_domain: Option<String>,
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
}

fn map_ch_row(r: AssetChRow) -> AssetRow {
    AssetRow {
        asset_type: r.asset_type,
        asset_type_name: asset_type_name(r.asset_type),
        asset_code: r.asset_code,
        issuer: r.issuer,
        contract_id: r.contract_id,
        name: r.name,
        symbol: r.symbol,
        decimals: r.decimals,
        total_supply: r.total_supply,
        holder_count: r.holder_count,
        icon_url: r.icon_url,
        deployed_at_ledger: r.deployed_at_ledger,
        issuer_home_domain: r.issuer_home_domain,
        issuer_id: r.issuer_id_key,
        contract_surrogate_id: r.contract_id_key,
    }
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
        "{ASSET_CH_SELECT} \
         WHERE 1{type_clause}{code_clause}{cursor_clause} \
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
    let rows = query.bind(params.limit).fetch_all::<AssetChRow>().await?;

    Ok(rows.into_iter().map(map_ch_row).collect())
}

// ---------------------------------------------------------------------------
// Detail — GET /v1/assets/:id (canonical 09), three resolution forms
// ---------------------------------------------------------------------------

/// Resolve by contract StrKey (SAC / Soroban / native XLM-SAC). PK seek on
/// `soroban_contracts.contract_id`, then the asset row by surrogate id.
pub async fn fetch_by_contract_id(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let sql = format!("{ASSET_CH_SELECT} WHERE sc.contract_id = ? LIMIT 1");
    let row = client
        .query(&sql)
        .bind(contract_id)
        .fetch_optional::<AssetChRow>()
        .await?;
    Ok(row.map(map_ch_row))
}

/// Resolve by `CODE-ISSUER` (classic credit / classic-wrap SAC). `issuer` is the
/// G-StrKey; the join resolves it to the surrogate `issuer_id`.
pub async fn fetch_by_code_issuer(
    client: &clickhouse::Client,
    asset_code: &str,
    issuer: &str,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let sql = format!("{ASSET_CH_SELECT} WHERE a.asset_code = ? AND iss.account_id = ? LIMIT 1");
    let row = client
        .query(&sql)
        .bind(asset_code)
        .bind(issuer)
        .fetch_optional::<AssetChRow>()
        .await?;
    Ok(row.map(map_ch_row))
}

/// Resolve the classic native XLM singleton (`asset_type = 0`). Native has no
/// composite identity, so it is addressed by the reserved `/assets/native`
/// token rather than a StrKey or CODE-ISSUER pair.
pub async fn fetch_native(
    client: &clickhouse::Client,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    let sql = format!("{ASSET_CH_SELECT} WHERE a.asset_type = 0 LIMIT 1");
    let row = client.query(&sql).fetch_optional::<AssetChRow>().await?;
    Ok(row.map(map_ch_row))
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
    source_account: Option<String>,
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
    asset_code: Option<&str>,
    asset_issuer_id: i64,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<AssetTxRow>, clickhouse::error::Error> {
    let has_classic = asset_code.is_some() && asset_issuer_id != 0;
    let has_contract = contract_surrogate_id != 0;
    if !has_classic && !has_contract {
        return Ok(Vec::new());
    }
    let (op, order) = keyset_sql_desc(direction);

    // Identity predicate. `asset_issuer_id` / `contract_surrogate_id` are i64
    // surrogates (no injection surface) → interpolated; `asset_code` is a DB
    // string value → bound.
    let mut branches: Vec<String> = Vec::new();
    if has_classic {
        branches.push(format!(
            "(oa.asset_code = ? AND oa.asset_issuer_id = {asset_issuer_id})"
        ));
    }
    if has_contract {
        branches.push(format!("(oa.contract_id = {contract_surrogate_id})"));
    }
    let predicate = branches.join(" OR ");

    // CH cursor only; a `Pg` variant never reaches here (the handler's
    // cross-source guard rejects it). Inlined i64 — no injection surface; the
    // clause is omitted entirely on the first page so no NULL is bound.
    let cursor_clause = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => format!(
            " AND (oa.ledger_sequence, oa.transaction_id) {op} ({ledger_sequence}, {tiebreak})"
        ),
        _ => String::new(),
    };

    // Step 1: driver seek. `operations_appearances` is ORDER BY
    // `(ledger_sequence, transaction_id, application_order)`; the identity
    // predicate is NOT the leading key, so CH walks descending ledgers until
    // the page fills (read-cost caveat in the module header). No FINAL — the
    // `LIMIT 1 BY` collapses re-ingest duplicates and the multi-op-per-tx
    // fan-out, so the page still yields `limit` distinct transactions.
    let driver_sql = format!(
        "SELECT oa.ledger_sequence AS ledger_sequence, oa.transaction_id AS transaction_id \
         FROM operations_appearances oa \
         WHERE ({predicate}) AND oa.ledger_sequence <= (SELECT max(sequence) FROM ledgers){cursor_clause} \
         ORDER BY oa.ledger_sequence {order}, oa.transaction_id {order} \
         LIMIT 1 BY oa.ledger_sequence, oa.transaction_id \
         LIMIT ?"
    );
    let mut driver = client.query(&driver_sql);
    if has_classic {
        driver = driver.bind(asset_code.expect("guarded by has_classic"));
    }
    // `limit` is the handler's `fetch_limit()` (already the peek +1).
    let key_rows = driver.bind(limit).fetch_all::<AssetTxKeyChRow>().await?;

    if key_rows.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<(i64, i64)> = key_rows
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

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
            nullIf(src.account_id, '') AS source_account, \
            t.fee_charged AS fee_charged, \
            t.successful AS successful, \
            t.operation_count AS operation_count, \
            t.has_soroban AS has_soroban, \
            l.closed_at AS created_at \
         FROM transactions t \
         LEFT JOIN accounts src ON src.id = t.source_id \
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
            source_account: row.source_account.unwrap_or_default(),
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
        assert_eq!(asset_type_name(2).as_deref(), Some("sac"));
        assert_eq!(asset_type_name(3).as_deref(), Some("soroban"));
        assert_eq!(asset_type_name(99), None);
    }
}
