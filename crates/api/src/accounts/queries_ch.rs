//! ClickHouse queries for the accounts endpoints (task 0243).
//!
//! Mirrors the PG path (`queries.rs`) one-for-one — same response shapes
//! (`AccountHeaderRow` / `AccountBalanceRow` / `AccountTxRow`), so the handler
//! stays backend-agnostic after the fetch. Notable CH-vs-PG divergences:
//!
//! - **`asset_type_name()` is a PG SQL function.** CH has no equivalent, so
//!   the `asset_type` → label mapping is done in Rust ([`asset_type_name`]),
//!   identical to the PG migration `20260422000000_enum_label_functions`.
//! - **Surrogate ids.** `accounts.id` / `account_balances_current.account_id` /
//!   `transaction_participants.account_id` are the `Int64` cityhash surrogate;
//!   resolved from the G-StrKey via the `accounts` primary key.
//! - **Account transactions span MANY ledger partitions** (an account is
//!   active over time), so the single-partition prune used by the global
//!   `/transactions` list does NOT apply here. Instead the page is driven off
//!   `transaction_participants` (ORDER BY `(account_id, ledger_sequence,
//!   transaction_id)` → an account-scoped primary-key seek), then the ≤ `limit`
//!   transaction rows are fetched by `(ledger_sequence, id) IN (keys)`
//!   (primary-key-prefix prune per ledger, multi-partition-safe) and re-ordered
//!   in Rust. No unpruned `transactions FINAL` join — that would merge the
//!   whole 3.6B-row table (the read_rows-quota trap fixed in the global list).

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{self, millis_to_utc};
use crate::common::cursor::{Direction, SortOrder, keyset_sql};
use crate::transactions::dto::TxListCursor;

use super::queries::{AccountBalanceRow, AccountHeaderRow, AccountTxRow};

/// `asset_type` SMALLINT → canonical label, matching the PG `asset_type_name`
/// function (`domain::AssetType`, 4 XDR variants). `None` for an out-of-range
/// code (the PG `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native".to_string()),
        1 => Some("credit_alphanum4".to_string()),
        2 => Some("credit_alphanum12".to_string()),
        3 => Some("pool_share".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Detail header — canonical 06 Statement A
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountHeaderChRow {
    id: i64,
    account_id: String,
    first_seen_ledger: i64,
    last_seen_ledger: i64,
    sequence_number: i64,
    home_domain: Option<String>,
}

/// `Ok(None)` → handler returns 404. PK seek on `accounts.account_id` + FINAL
/// (state table; latest version).
pub async fn fetch_account(
    client: &clickhouse::Client,
    account_strkey: &str,
) -> Result<Option<AccountHeaderRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                a.id, \
                a.account_id, \
                a.first_seen_ledger, \
                a.last_seen_ledger, \
                a.sequence_number, \
                a.home_domain \
             FROM accounts a FINAL \
             WHERE a.account_id = ? \
             LIMIT 1",
        )
        .bind(account_strkey)
        .fetch_optional::<AccountHeaderChRow>()
        .await?;

    Ok(row.map(|r| AccountHeaderRow {
        id: r.id,
        account_id: r.account_id,
        first_seen_ledger: r.first_seen_ledger,
        last_seen_ledger: r.last_seen_ledger,
        sequence_number: r.sequence_number,
        home_domain: r.home_domain,
    }))
}

// ---------------------------------------------------------------------------
// Detail balances — canonical 06 Statement B
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountBalanceChRow {
    asset_type: i16,
    asset_code: Option<String>,
    asset_issuer: Option<String>,
    balance: String,
    last_updated_ledger: i64,
}

/// `account_id` is the surrogate from [`fetch_account`]. Leading-PK seek on
/// `account_balances_current` (ORDER BY `(account_id, …)`).
pub async fn fetch_balances(
    client: &clickhouse::Client,
    account_id: i64,
) -> Result<Vec<AccountBalanceRow>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT \
                abc.asset_type                  AS asset_type, \
                nullIf(abc.asset_code, '')      AS asset_code, \
                nullIf(iss.account_id, '')      AS asset_issuer, \
                toString(abc.balance)           AS balance, \
                abc.last_updated_ledger         AS last_updated_ledger \
             FROM account_balances_current abc FINAL \
             LEFT JOIN accounts iss ON iss.id = abc.issuer_id \
             WHERE abc.account_id = ? \
             ORDER BY abc.asset_type, abc.asset_code, iss.account_id \
             LIMIT 1 BY (abc.asset_type, abc.asset_code, abc.issuer_id)",
        )
        .bind(account_id)
        .fetch_all::<AccountBalanceChRow>()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| AccountBalanceRow {
            asset_type_name: asset_type_name(r.asset_type),
            asset_type: r.asset_type,
            asset_code: r.asset_code,
            asset_issuer: r.asset_issuer,
            balance: r.balance,
            last_updated_ledger: r.last_updated_ledger,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Transactions — canonical 07 (two-step, multi-partition-safe)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ParticipantKeyRow {
    ledger_sequence: i64,
    transaction_id: i64,
}

#[derive(Debug, Row, Deserialize)]
struct AccountTxPageChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    application_order: i16,
    source_account: Option<String>,
    fee_charged: i64,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at: i64,
}

/// `account_id` is the surrogate from [`fetch_account`]; caller passes
/// `limit + 1` (the peek row drives forward-continuation detection). The CH
/// cursor keys on `(ledger_sequence, transaction_id)` — a `Pg`-variant cursor
/// never reaches here (`list_account_transactions` rejects a cross-datasource
/// cursor before dispatch), so the `_` arm only ever means "first page".
pub async fn fetch_transactions(
    client: &clickhouse::Client,
    account_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountTxRow>, clickhouse::error::Error> {
    let (cursor_ledger, cursor_tiebreak): (Option<i64>, Option<i64>) = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => (Some(*ledger_sequence), Some(*tiebreak)),
        _ => (None, None),
    };
    let (op, order) = keyset_sql(sort, direction);

    // Inline the cursor bounds rather than `.bind()`-ing them: the clickhouse
    // 0.15 bound-parameter path returns an empty result when `None` is bound
    // into a tuple keyset comparison — the same defect that forced the
    // transactions B/C statements to inline (a bound first page silently
    // returned 0 rows). Values are i64 / None→NULL, so no injection surface.
    let cl = cursor_ledger.map_or_else(|| "NULL".to_string(), |v| v.to_string());
    let ct = cursor_tiebreak.map_or_else(|| "NULL".to_string(), |v| v.to_string());

    // Step 1: account-scoped driver seek. `account_id` is the leading PK of
    // `transaction_participants`. FINAL is dropped: on a hot account it merges
    // the account's rows across every part — measured 1.16B vs 327M rows read
    // (3.5x), and one such FINAL page is ~11% of the hourly read_rows quota.
    // `LIMIT 1 BY (ledger_sequence, transaction_id)` collapses any rare
    // re-ingest duplicate, so the page still yields `limit` distinct keys
    // (box-verified: 21/21 distinct on the hottest account).
    let driver_sql = format!(
        "SELECT tp.ledger_sequence AS ledger_sequence, tp.transaction_id AS transaction_id \
         FROM transaction_participants tp \
         WHERE tp.account_id = ? \
           AND ({cl} IS NULL OR (tp.ledger_sequence, tp.transaction_id) {op} ({cl}, {ct})) \
         ORDER BY tp.ledger_sequence {order}, tp.transaction_id {order} \
         LIMIT 1 BY tp.ledger_sequence, tp.transaction_id \
         LIMIT ?"
    );
    let key_rows = client
        .query(&driver_sql)
        .bind(account_id)
        .bind(limit)
        .fetch_all::<ParticipantKeyRow>()
        .await?;

    if key_rows.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<(i64, i64)> = key_rows
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

    // Step 2: fetch the ≤limit transaction rows by `(ledger_sequence, id) IN
    // (keys)` (primary-key-prefix prune per ledger; spans partitions safely),
    // concurrently with the operation_types aggregate for the same keys.
    // Keys are `i64`, inlined directly — no injection surface.
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
            t.ledger_sequence, \
            t.application_order, \
            nullIf(src.account_id, '') AS source_account, \
            t.fee_charged, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            l.closed_at AS created_at \
         FROM transactions t \
         LEFT JOIN accounts src ON src.id = t.source_id \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions})"
    );
    let (page_rows, aggregates) = tokio::join!(
        client.query(&page_sql).fetch_all::<AccountTxPageChRow>(),
        ch::fetch_tx_list_aggregates(client, &keys),
    );
    let page_rows = page_rows?;
    let aggregates = aggregates?;

    // Step 3: index page rows by id (a re-ingested tx collapses — values are
    // immutable), then emit in the driver's keyset order, merging
    // operation_types. `contract_ids` from the helper is intentionally unused:
    // the account-transaction item carries only `operation_types`.
    let mut by_id: HashMap<i64, AccountTxPageChRow> = HashMap::with_capacity(page_rows.len());
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
        out.push(AccountTxRow {
            id: row.id,
            hash: row.hash,
            ledger_sequence: row.ledger_sequence,
            application_order: row.application_order,
            source_account: row.source_account.unwrap_or_default(),
            fee_charged: row.fee_charged,
            successful: row.successful,
            operation_count: row.operation_count,
            has_soroban: row.has_soroban,
            operation_types,
            created_at: millis_to_utc(row.created_at),
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
        assert_eq!(asset_type_name(1).as_deref(), Some("credit_alphanum4"));
        assert_eq!(asset_type_name(2).as_deref(), Some("credit_alphanum12"));
        assert_eq!(asset_type_name(3).as_deref(), Some("pool_share"));
        assert_eq!(asset_type_name(99), None);
    }
}
