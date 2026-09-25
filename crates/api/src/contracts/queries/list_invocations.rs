//! `GET /v1/contracts/:id/invocations` — the contract's Invocations tab.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

#[derive(Debug)]
pub struct InvocationAppearanceRow {
    /// The transaction's position in its ledger — the cursor's tie-break.
    pub application_order: i16,
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
    pub caller_account: Option<String>,
    pub successful: bool,
}

// ---------------------------------------------------------------------------
// Invocations — canonical 13 (two-step, multi-partition-safe)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct InvocationKeyRow {
    ledger_sequence: i64,
    application_order: i16,
    caller_id: Option<i64>,
}

#[derive(Debug, Row, Deserialize)]
struct TxMetaChRow {
    ledger_sequence: i64,
    application_order: i16,
    hash: String,
    successful: bool,
    created_at: i64,
}

/// `contract_surrogate_id` is from [`fetch_contract`]; caller passes
/// `limit + 1`. Driven off `contract_activity` (leading-PK seek on
/// `contract_id`), keeping only the rows where the contract was invoked, then
/// the page's transaction header columns (`hash` / `successful` / `closed_at`)
/// are fetched by `(ledger_sequence, application_order) IN (keys)` and merged.
/// The cursor is the transaction's position (task 0586), so a page lists the
/// contract's invocations in execution order.
pub async fn fetch_invocation_appearances(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Inline the cursor bound rather than `.bind()`-ing it: the clickhouse
    // 0.15 bound-parameter path returns an empty result when `None` is bound
    // into a tuple keyset comparison (the same defect that forced transactions
    // B/C to inline). Integers only, no injection surface.
    // The handler has already refused a cursor of another keyset.
    let keyset = match cursor {
        Some(TxListCursor::ChPosition {
            ledger_sequence,
            application_order,
        }) => format!(
            " AND (ledger_sequence, application_order) {op} ({ledger_sequence}, {application_order})"
        ),
        _ => String::new(),
    };

    // Step 1: contract-scoped driver seek. `contract_id` is the leading PK of
    // `contract_activity`, so the query reads only this contract's rows;
    // `invocation_count > 0` keeps the invoked ones (a touched-only row counts
    // 0). The page LIMIT sits INSIDE the subquery so the read stops at it in
    // key order; `LIMIT 1 BY` on the same level as the LIMIT disables that
    // (measured on a SAC with 10 M weekly invocations: 22.3 M rows read flat,
    // 4.4 M nested). No FINAL — with it CH merges the contract's rows across
    // every part (~38× read amplification, measured on the invocations table);
    // the outer `LIMIT 1 BY` collapses a rare re-ingest duplicate instead.
    let driver_sql = format!(
        "SELECT m.ledger_sequence AS ledger_sequence, \
                m.application_order AS application_order, \
                m.caller_id AS caller_id \
         FROM ( \
            SELECT ledger_sequence, application_order, caller_id \
            FROM contract_activity \
            WHERE contract_id = ? \
              AND invocation_count > 0 \
              AND ledger_sequence <= (SELECT max(sequence) FROM ledgers){keyset} \
            ORDER BY ledger_sequence {order}, application_order {order} \
            LIMIT ? \
         ) m \
         LIMIT 1 BY m.ledger_sequence, m.application_order"
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

    // Step 2: fetch the transaction header columns for the page's positions.
    let in_tuples = key_rows
        .iter()
        .map(|r| format!("({},{})", r.ledger_sequence, r.application_order))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = key_rows
        .iter()
        .map(|r| r.ledger_sequence / 500_000)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let page_sql = format!(
        "SELECT \
            t.ledger_sequence AS ledger_sequence, \
            t.application_order AS application_order, \
            lower(hex(t.hash)) AS hash, \
            t.successful, \
            l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.application_order) IN ({in_tuples}) \
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

    let mut tx_by_position: HashMap<(i64, i16), TxMetaChRow> =
        HashMap::with_capacity(tx_rows.len());
    for row in tx_rows {
        tx_by_position.insert((row.ledger_sequence, row.application_order), row);
    }

    // Emit in driver keyset order, merging the transaction header columns. A
    // key whose transaction row is somehow absent is skipped (should not occur
    // — an invocation always has its parent transaction).
    let mut out = Vec::with_capacity(key_rows.len());
    for key in &key_rows {
        let Some(tx) = tx_by_position.get(&(key.ledger_sequence, key.application_order)) else {
            continue;
        };
        out.push(InvocationAppearanceRow {
            application_order: key.application_order,
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
