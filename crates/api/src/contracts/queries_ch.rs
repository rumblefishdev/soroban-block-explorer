//! ClickHouse queries for the contracts endpoints (task 0243).
//!
//! Mirrors the PG path (`queries.rs`) — same response shapes, so the handler
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
//! many events). `EventItem.amount` (a vestigial fold-count, not surfaced by the
//! FE) is `1` on CH — each row is one unfolded event.
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

use crate::common::ch::millis_to_utc;
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

use super::dto::{EventCursor, EventItem};
use super::queries::{
    ContractListRow, ContractRow, InterfaceRow, InvocationAppearanceRow,
    ResolvedContractsListParams, STATS_WINDOW,
};

/// `contract_type` SMALLINT → label, matching the PG `contract_type_name`
/// function. `None` for an out-of-range code (PG `CASE` returns NULL).
fn contract_type_name(contract_type: i16) -> Option<String> {
    match contract_type {
        0 => Some("token".to_string()),
        1 => Some("other".to_string()),
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
    deployer: Option<String>,
    deployed_at_ledger: Option<i64>,
}

#[derive(Debug, Row, Deserialize)]
struct InvocationCountChRow {
    contract_id: i64,
    recent_invocations: u64,
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
///   `contract_id` + `name` (`positionCaseInsensitive`). A full scan of the
///   (small) contracts table, NOT tokenized search — close enough for the
///   explorer's id/name lookup, documented divergence.
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
        " AND (positionCaseInsensitive(sc.contract_id, ?) > 0 \
              OR positionCaseInsensitive(ifNull(sc.name, ''), ?) > 0)"
    } else {
        ""
    };

    let list_sql = format!(
        "SELECT \
            sc.id                           AS id, \
            sc.contract_id                  AS contract_id, \
            sc.contract_type                AS contract_type, \
            sc.is_sac                       AS is_sac, \
            nullIf(deployer.account_id, '') AS deployer, \
            sc.deployed_at_ledger           AS deployed_at_ledger \
         FROM soroban_contracts sc FINAL \
         LEFT JOIN accounts deployer ON deployer.id = sc.deployer_id \
         WHERE 1{cursor_clause}{type_clause}{q_clause} \
         ORDER BY sc.id {order} \
         LIMIT ?"
    );

    let mut list_query = client.query(&list_sql);
    if let Some(q) = &params.q {
        list_query = list_query.bind(q).bind(q);
    }
    // `params.limit` is the handler's `fetch_limit()` (already the peek +1).
    let list_rows = list_query
        .bind(params.limit)
        .fetch_all::<ContractListChRow>()
        .await?;

    if list_rows.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: invocation counts in the STATS_WINDOW for the page's ids. Window
    // applied via a `ledgers.closed_at` JOIN, floored first by a
    // `ledger_sequence` lower bound so the seek stays on the appearance PK
    // prefix (same shape as `fetch_contract_stats`). `FINAL` matches the detail
    // stat so re-ingest duplicates collapse identically.
    let ids = list_rows
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let days: i64 = STATS_WINDOW
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(7);
    let ledger_floor = days.saturating_mul(LEDGERS_PER_DAY);
    let count_sql = format!(
        "SELECT \
            sia.contract_id                  AS contract_id, \
            toUInt64(count())                AS recent_invocations \
         FROM soroban_invocations_appearances sia FINAL \
         INNER JOIN ledgers l ON l.sequence = sia.ledger_sequence \
         WHERE sia.contract_id IN ({ids}) \
           AND sia.ledger_sequence >= (SELECT max(sequence) FROM ledgers) - {ledger_floor} \
           AND l.closed_at >= now64() - INTERVAL {days} DAY \
         GROUP BY sia.contract_id"
    );
    let count_rows = client
        .query(&count_sql)
        .fetch_all::<InvocationCountChRow>()
        .await?;
    let counts: HashMap<i64, i64> = count_rows
        .into_iter()
        .map(|r| (r.contract_id, r.recent_invocations as i64))
        .collect();

    Ok(list_rows
        .into_iter()
        .map(|r| ContractListRow {
            recent_invocations: counts.get(&r.id).copied().unwrap_or(0),
            contract_type_name: r.contract_type.and_then(contract_type_name),
            id: r.id,
            contract_id: r.contract_id,
            contract_type: r.contract_type,
            is_sac: r.is_sac,
            deployer: r.deployer,
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
    deployer: Option<String>,
    deployed_at_ledger: Option<i64>,
    contract_type: Option<i16>,
    is_sac: bool,
}

pub async fn fetch_contract(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<ContractRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                sc.id, \
                sc.contract_id, \
                lower(hex(sc.wasm_hash))               AS wasm_hash, \
                nullIf(sc.wasm_uploaded_at_ledger, 0)  AS wasm_uploaded_at_ledger, \
                nullIf(deployer.account_id, '')        AS deployer, \
                sc.deployed_at_ledger                  AS deployed_at_ledger, \
                sc.contract_type                       AS contract_type, \
                sc.is_sac                              AS is_sac \
             FROM soroban_contracts sc FINAL \
             LEFT JOIN accounts deployer ON deployer.id = sc.deployer_id \
             WHERE sc.contract_id = ? \
             LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<ContractHeaderChRow>()
        .await?;

    Ok(row.map(|r| ContractRow {
        id: r.id,
        contract_id: r.contract_id,
        wasm_hash: r.wasm_hash,
        wasm_uploaded_at_ledger: r.wasm_uploaded_at_ledger,
        deployer: r.deployer,
        deployed_at_ledger: r.deployed_at_ledger,
        contract_type_name: r.contract_type.and_then(contract_type_name),
        contract_type: r.contract_type,
        is_sac: r.is_sac,
    }))
}

// ---------------------------------------------------------------------------
// Bounded-window stats — canonical 11 Statement B
// ---------------------------------------------------------------------------

/// ~Ledgers per day at the ~5 s mainnet cadence — used to bound the
/// `(contract_id, ledger_sequence)` seek to the recent window before the exact
/// `closed_at` predicate refines it, so a hot contract's full invocation
/// history is never scanned for the 7-day stat.
const LEDGERS_PER_DAY: i64 = 17_280;

#[derive(Debug, Row, Deserialize)]
struct StatsChRow {
    recent_invocations: u64,
    recent_unique_callers: u64,
}

/// `window` is the echoed label (e.g. `"7 days"`); its leading integer is the
/// day count. CH `soroban_invocations_appearances` has no `created_at`, so the
/// window is applied via a JOIN to `ledgers.closed_at`, bounded first by a
/// `ledger_sequence` floor so the seek stays on the primary-key prefix.
pub async fn fetch_contract_stats(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    window: &str,
) -> Result<(i64, i64, i64, String), clickhouse::error::Error> {
    let days: i64 = window
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(7);
    let ledger_floor = days.saturating_mul(LEDGERS_PER_DAY);

    // `days` / `ledger_floor` are derived from the operator-controlled window
    // label, not user input — safe to interpolate.
    let sql = format!(
        "SELECT \
            toUInt64(count())                       AS recent_invocations, \
            toUInt64(uniqExact(sia.caller_id))      AS recent_unique_callers \
         FROM soroban_invocations_appearances sia FINAL \
         INNER JOIN ledgers l ON l.sequence = sia.ledger_sequence \
         WHERE sia.contract_id = ? \
           AND sia.ledger_sequence >= (SELECT max(sequence) FROM ledgers) - {ledger_floor} \
           AND l.closed_at >= now64() - INTERVAL {days} DAY"
    );
    let row = client
        .query(&sql)
        .bind(contract_surrogate_id)
        .fetch_one::<StatsChRow>()
        .await?;

    Ok((
        row.recent_invocations as i64,
        row.recent_unique_callers as i64,
        0,
        window.to_string(),
    ))
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
    caller_account: Option<String>,
    amount: i32,
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
            nullIf(caller.account_id, '') AS caller_account, \
            m.amount AS amount \
         FROM ( \
            SELECT ledger_sequence, transaction_id, caller_id, amount \
            FROM soroban_invocations_appearances \
            WHERE contract_id = ? \
              AND ({cl} IS NULL OR (ledger_sequence, transaction_id) {op} ({cl}, {ct})) \
            ORDER BY ledger_sequence {order}, transaction_id {order} \
            LIMIT ? \
         ) m \
         LEFT JOIN accounts caller ON caller.id = m.caller_id \
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
    let tx_rows = client.query(&page_sql).fetch_all::<TxMetaChRow>().await?;

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
            caller_account: key.caller_account.clone(),
            amount: key.amount,
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
            // CH is per-event (unfolded); the PG fold-count is vestigial and not
            // surfaced by the FE — each CH row is one event.
            amount: 1,
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

    let sql = format!(
        "SELECT \
            se.ledger_sequence              AS ledger_sequence, \
            se.transaction_id               AS transaction_id, \
            se.event_index                  AS event_index, \
            se.event_type                   AS event_type, \
            se.topics_xdr                   AS topics_xdr, \
            se.data_xdr                     AS data_xdr, \
            lower(hex(t.hash))              AS transaction_hash, \
            t.successful                    AS successful, \
            l.closed_at                     AS created_at \
         FROM soroban_events se FINAL \
         JOIN transactions t \
              ON t.id = se.transaction_id AND t.ledger_sequence = se.ledger_sequence \
         INNER JOIN ledgers l ON l.sequence = se.ledger_sequence \
         WHERE se.contract_id = ?{cursor_clause} \
         ORDER BY se.ledger_sequence {order}, se.transaction_id {order}, se.event_index {order} \
         LIMIT 1 BY se.ledger_sequence, se.transaction_id, se.event_index \
         LIMIT ?"
    );

    let rows = client
        .query(&sql)
        .bind(contract_surrogate_id)
        .bind(limit)
        .fetch_all::<EventChRow>()
        .await?;

    Ok(rows.into_iter().map(map_event_row).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_type_name_matches_pg_function() {
        assert_eq!(contract_type_name(0).as_deref(), Some("token"));
        assert_eq!(contract_type_name(1).as_deref(), Some("other"));
        assert_eq!(contract_type_name(2), None);
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
        assert_eq!(ev.item.amount, 1); // CH per-event (unfolded)
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
