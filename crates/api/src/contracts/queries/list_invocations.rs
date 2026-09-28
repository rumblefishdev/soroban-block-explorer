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
    pub transaction_id: i64,
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
        Some(TxListCursor::ChSurrogate {
            ledger_sequence,
            transaction_id,
        }) => (Some(*ledger_sequence), Some(*transaction_id)),
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
