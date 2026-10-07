//! `GET /v1/contracts/:id/events` — the contract's Events tab.

use clickhouse::Row;
use serde::Deserialize;

use domain::ContractEventType;

use crate::common::ch::millis_to_utc;
use crate::common::cursor::{Direction, keyset_sql_desc};

use super::super::dto::{EventCursor, EventItem};

// ---------------------------------------------------------------------------
// Events — GET /v1/contracts/:id/events (full-content CH read)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct EventChRow {
    ledger_sequence: i64,
    transaction_index: u32,
    operation_index: u16,
    event_index: u32,
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
    transaction_index: u32,
    operation_index: u16,
    event_index: u32,
    application_order: i16,
    event_type: i16,
    topics_xdr: String,
    data_xdr: String,
}

/// Step-2 resolve row: `(ledger_sequence, application_order)` →
/// `(hash, successful)`.
#[derive(Debug, Row, Deserialize)]
struct EventTxRow {
    ledger_sequence: i64,
    application_order: i16,
    /// Already `lower(hex())` in the query.
    hash: String,
    successful: bool,
    /// Parent ledger `closed_at`, joined in — CH has no
    /// `transactions.created_at` (ADR 0044 §5.2).
    created_at: i64,
}

/// A decoded event row + its rpc id parts (the cursor, which the wire carries
/// only as the `id` string). The handler finalises the page over these, builds
/// the `EventCursor::ChEventId` from the boundary row, then maps to
/// `EventItem`.
pub struct ChEvent {
    pub transaction_index: u32,
    pub operation_index: u16,
    pub event_index: u32,
    pub item: EventItem,
}

/// The indexer writes `topics_xdr` / `data_xdr` with `serde_json` and
/// `event_type` from an exhaustive match, so a value that does not decode is a
/// broken row: the page fails rather than show an invented one.
fn map_event_row(r: EventChRow) -> Result<ChEvent, clickhouse::error::Error> {
    let id = xdr_parser::EventId {
        ledger_sequence: u32::try_from(r.ledger_sequence)
            .expect("a ledger sequence is a u32 by protocol"),
        transaction_index: r.transaction_index,
        operation_index: r.operation_index,
        event_index: r.event_index,
    };
    let id = id.to_rpc_string();
    // `topics` is a JSON array of ScVals; mirror the PG `expand_events` shaping
    // (array → its elements, scalar → singleton).
    let topics = match serde_json::from_str::<serde_json::Value>(&r.topics_xdr) {
        Ok(serde_json::Value::Array(a)) => a,
        Ok(other) => vec![other],
        Err(e) => return Err(broken_event(&id, &format!("topics are not JSON: {e}"))),
    };
    let data = serde_json::from_str::<serde_json::Value>(&r.data_xdr)
        .map_err(|e| broken_event(&id, &format!("data is not JSON: {e}")))?;
    let event_type = match ContractEventType::try_from(r.event_type) {
        Ok(t) => t.to_string(),
        Err(_) => {
            return Err(broken_event(
                &id,
                &format!("unknown event_type {}", r.event_type),
            ));
        }
    };
    Ok(ChEvent {
        transaction_index: r.transaction_index,
        operation_index: r.operation_index,
        event_index: r.event_index,
        item: EventItem {
            id,
            transaction_hash: r.transaction_hash,
            ledger_sequence: r.ledger_sequence,
            successful: r.successful,
            created_at: millis_to_utc(r.created_at),
            event_type,
            topics,
            data,
        },
    })
}

/// A stored event that breaks a rule the indexer keeps. The handler logs it
/// and answers 500.
fn broken_event(id: &str, what: &str) -> clickhouse::error::Error {
    clickhouse::error::Error::Custom(format!("soroban_events row {id}: {what}"))
}

/// `contract_surrogate_id` is from [`fetch_contract`]; caller passes the
/// handler's `fetch_limit()` (already the peek `+1`). Two statements: the page
/// by a contract-leading PK seek on `soroban_events`, then its transactions by
/// position. No `FINAL` on either; `LIMIT 1 BY` collapses duplicate rows. The
/// cursor keys on the rpc event id
/// `(ledger_sequence, transaction_index, operation_index, event_index)`.
pub async fn fetch_events(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&EventCursor>,
    direction: Direction,
) -> Result<Vec<ChEvent>, clickhouse::error::Error> {
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
    // full-key `LIMIT 1 BY` on the rpc id already collapses re-ingest
    // duplicates, and every projected column is immutable across
    // ReplacingMergeTree versions, so a non-FINAL read returns identical rows.
    // `LIMIT 1 BY` does not stop the read at `LIMIT`, so it runs on the key
    // columns alone — see `events_page_sql`.
    let raw = client
        .query(&events_page_sql(cursor, direction))
        .bind(contract_surrogate_id)
        .bind(contract_surrogate_id)
        .bind(limit)
        .fetch_all::<EventPageRow>()
        .await?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: resolve the page's `transaction_hash` / `successful` / `closed_at`
    // with a seek on the `transactions` key `(ledger_sequence,
    // application_order)` instead of full-table hash joins (task 0290). The
    // event row names its transaction by position — a fee refund's rpc id
    // carries a sentinel, not the transaction. No `FINAL` (a transaction is
    // immutable, so a dup version is identical).
    //
    // `closed_at` rides along on the `ledgers` join rather than costing its own
    // statement (task 0446).
    let mut positions: Vec<(i64, i16)> = raw
        .iter()
        .map(|r| (r.ledger_sequence, r.application_order))
        .collect();
    positions.sort_unstable();
    positions.dedup();

    let txs: std::collections::HashMap<(i64, i16), (String, bool, i64)> = client
        .query(&event_transactions_sql(&positions))
        .fetch_all::<EventTxRow>()
        .await?
        .into_iter()
        .map(|r| {
            (
                (r.ledger_sequence, r.application_order),
                (r.hash, r.successful, r.created_at),
            )
        })
        .collect();

    // Rebuild full event rows in page order, then map. Every event's
    // transaction is written in the same ledger batch, and step 1 reads only
    // ledgers whose `ledgers` row (written last) has landed, so a missing
    // transaction is a broken row, not a gap to paper over.
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        let Some((transaction_hash, successful, created_at)) =
            txs.get(&(r.ledger_sequence, r.application_order)).cloned()
        else {
            return Err(clickhouse::error::Error::Custom(format!(
                "soroban_events row at ledger {} application_order {}: no transaction",
                r.ledger_sequence, r.application_order
            )));
        };
        out.push(map_event_row(EventChRow {
            ledger_sequence: r.ledger_sequence,
            transaction_index: r.transaction_index,
            operation_index: r.operation_index,
            event_index: r.event_index,
            event_type: r.event_type,
            topics_xdr: r.topics_xdr,
            data_xdr: r.data_xdr,
            transaction_hash,
            successful,
            created_at,
        })?);
    }
    Ok(out)
}

/// Step 1 of [`fetch_events`]: one page of a contract's events in rpc id
/// order. Binds `contract_id`, `contract_id`, `limit`. The cursor is inlined (integers —
/// no injection surface) and omitted on the first page, so no NULL is bound
/// into the tuple keyset (the clickhouse 0.15 None-in-tuple defect).
fn events_page_sql(cursor: Option<&EventCursor>, direction: Direction) -> String {
    let (op, order) = keyset_sql_desc(direction);
    let cursor_clause = match cursor {
        Some(EventCursor::ChEventId {
            ledger_sequence,
            transaction_index,
            operation_index,
            event_index,
        }) => format!(
            " AND (se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index) {op} \
             ({ledger_sequence}, {transaction_index}, {operation_index}, {event_index})"
        ),
        None => String::new(),
    };
    // The page's keys first, on the key columns alone; the payload only for
    // those keys. One statement reading both would load `topics_xdr` /
    // `data_xdr` for every row `LIMIT 1 BY` walks — the native SAC's first page
    // read 1.2 GiB in ~0.5 s with 2.2 GiB of memory, against 0.6 GiB, ~0.15 s
    // and 0.5 GiB this way (2026-09-22). `LIMIT 1 BY` stays exact, so the page
    // is never short of `limit` while more events exist.
    format!(
        "SELECT \
            e.ledger_sequence               AS ledger_sequence, \
            e.transaction_index             AS transaction_index, \
            e.operation_index               AS operation_index, \
            e.event_index                   AS event_index, \
            e.application_order             AS application_order, \
            e.event_type                    AS event_type, \
            e.topics_xdr                    AS topics_xdr, \
            e.data_xdr                      AS data_xdr \
         FROM soroban_events e \
         WHERE e.contract_id = ? \
           AND (e.ledger_sequence, e.transaction_index, e.operation_index, e.event_index) IN ( \
             SELECT se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index \
             FROM soroban_events se \
             WHERE se.contract_id = ? AND se.ledger_sequence <= (SELECT max(sequence) FROM ledgers){cursor_clause} \
             ORDER BY se.ledger_sequence {order}, se.transaction_index {order}, se.operation_index {order}, se.event_index {order} \
             LIMIT 1 BY se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index \
             LIMIT ?) \
         ORDER BY e.ledger_sequence {order}, e.transaction_index {order}, e.operation_index {order}, e.event_index {order} \
         LIMIT 1 BY e.ledger_sequence, e.transaction_index, e.operation_index, e.event_index"
    )
}

/// Step 2 of [`fetch_events`]: the page's transactions by position.
fn event_transactions_sql(positions: &[(i64, i16)]) -> String {
    let in_pairs = positions
        .iter()
        .map(|(ledger, order)| format!("({ledger},{order})"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "SELECT t.ledger_sequence AS ledger_sequence, t.application_order AS application_order, \
                lower(hex(t.hash)) AS hash, t.successful AS successful, \
                l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.application_order) IN ({in_pairs}) \
         LIMIT 1 BY t.ledger_sequence, t.application_order"
    )
}

#[cfg(test)]
mod ch_tests;

#[cfg(test)]
mod tests;
