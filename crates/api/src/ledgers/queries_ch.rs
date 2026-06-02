//! ClickHouse queries for the ledgers endpoints.
//!
//! The public response shape intentionally mirrors the PostgreSQL path:
//! `GET /v1/ledgers/:sequence` still embeds full `TransactionListItem`
//! rows because the frontend reuses the global transactions table on
//! ledger detail pages. CH does not store `transactions.created_at`, so
//! the ledger `closed_at` value is joined in and used as the API timestamp.
//! For embedded transactions, `TsIdCursor.id` carries `application_order`
//! on the CH path: unlike PostgreSQL's `BIGSERIAL`, CH `transactions.id`
//! is a deterministic hash surrogate and must not define in-ledger order.

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{self, millis_to_utc};
use crate::common::cursor::{Direction, TsIdCursor, direction_sql};

use super::dto::LedgerListItem;
use super::queries::{LedgerDetailRow, LedgerTxRow};

#[derive(Debug, Row, Deserialize)]
struct LedgerListRow {
    sequence: i64,
    hash: String,
    closed_at: i64,
    protocol_version: i32,
    transaction_count: i32,
    base_fee: i64,
}

impl From<LedgerListRow> for LedgerListItem {
    fn from(row: LedgerListRow) -> Self {
        Self {
            sequence: row.sequence,
            hash: row.hash,
            closed_at: millis_to_utc(row.closed_at),
            protocol_version: row.protocol_version,
            transaction_count: row.transaction_count,
            base_fee: row.base_fee,
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct LedgerDetailChRow {
    sequence: i64,
    hash: String,
    closed_at: i64,
    protocol_version: i32,
    transaction_count: i32,
    base_fee: i64,
    prev_sequence: Option<i64>,
    next_sequence: Option<i64>,
}

impl From<LedgerDetailChRow> for LedgerDetailRow {
    fn from(row: LedgerDetailChRow) -> Self {
        Self {
            sequence: row.sequence,
            hash: row.hash,
            closed_at: millis_to_utc(row.closed_at),
            protocol_version: row.protocol_version,
            transaction_count: row.transaction_count,
            base_fee: row.base_fee,
            prev_sequence: row.prev_sequence,
            next_sequence: row.next_sequence,
        }
    }
}

/// One embedded-transaction page row — slim base columns only.
/// `operation_types` + `contract_ids` are fetched separately via
/// [`ch::fetch_tx_list_aggregates`] and merged by the surrogate tx id
/// (CH 26.3 cannot compute them inline with correlated subqueries).
#[derive(Debug, Row, Deserialize)]
struct LedgerTxPageChRow {
    /// `transactions.id` hash surrogate — the aggregate join key. NOT the
    /// API cursor id (CH `id` is not apply-order; see `into_ledger_tx_row`).
    tx_surrogate: i64,
    hash: String,
    ledger_sequence: i64,
    application_order: i16,
    source_account: Option<String>,
    fee_charged: i64,
    inner_tx_hash: Option<String>,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at: i64,
}

impl LedgerTxPageChRow {
    /// Merge this page row with its pre-fetched aggregates into `LedgerTxRow`.
    ///
    /// `LedgerTxRow.id` carries `application_order` on the CH path (the
    /// `TsIdCursor.id` tie-break for embedded-tx pagination): CH
    /// `transactions.id` is a deterministic hash surrogate and must not
    /// define in-ledger order, so the cursor keys on `application_order`.
    fn into_ledger_tx_row(self, agg: ch::TxListAggregates) -> LedgerTxRow {
        LedgerTxRow {
            id: i64::from(self.application_order),
            hash: self.hash,
            ledger_sequence: self.ledger_sequence,
            application_order: self.application_order,
            source_account: self.source_account.filter(|s| !s.is_empty()),
            fee_charged: self.fee_charged,
            inner_tx_hash: self.inner_tx_hash.filter(|s| !s.is_empty()),
            successful: self.successful,
            operation_count: self.operation_count,
            has_soroban: self.has_soroban,
            operation_types: agg.operation_types,
            contract_ids: agg.contract_ids,
            created_at: millis_to_utc(self.created_at),
        }
    }
}

pub async fn fetch_list(
    client: &clickhouse::Client,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    direction: Direction,
) -> Result<Vec<LedgerListItem>, clickhouse::error::Error> {
    let cursor_sequence = cursor.map(|c| c.id);
    let (op, order) = direction_sql(direction);

    // `ledgers` is ORDER BY `sequence`. Paginating by `closed_at` (as the PG
    // path does) forces a full ~12M-row scan + sort on every page (measured),
    // and this endpoint is polled, so that scan recurs and eats the
    // `read_rows` quota. `sequence` is monotonic with `closed_at` (a later
    // ledger closes later), so ordering + keying on `sequence` alone yields the
    // identical page in primary-key read-in-order — a handful of rows per page.
    // `sequence` is unique, so it needs no tie-break. The cursor still carries
    // `closed_at` for the wire `ts`, but the keyset no longer reads it.
    let sql = format!(
        "SELECT \
            l.sequence, \
            lower(hex(l.hash)) AS hash, \
            l.closed_at, \
            l.protocol_version, \
            l.transaction_count, \
            l.base_fee \
        FROM ledgers l \
        WHERE ? IS NULL OR l.sequence {op} ? \
        ORDER BY l.sequence {order} \
        LIMIT ?",
    );

    let rows = client
        .query(&sql)
        .bind(cursor_sequence)
        .bind(cursor_sequence)
        .bind(limit)
        .fetch_all::<LedgerListRow>()
        .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn fetch_by_sequence(
    client: &clickhouse::Client,
    sequence: i64,
) -> Result<Option<LedgerDetailRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                l.sequence, \
                lower(hex(l.hash)) AS hash, \
                l.closed_at, \
                l.protocol_version, \
                l.transaction_count, \
                l.base_fee, \
                (SELECT sequence FROM ledgers WHERE sequence < ? ORDER BY sequence DESC LIMIT 1) AS prev_sequence, \
                (SELECT sequence FROM ledgers WHERE sequence > ? ORDER BY sequence ASC LIMIT 1) AS next_sequence \
            FROM ledgers l \
            WHERE l.sequence = ?",
        )
        .bind(sequence)
        .bind(sequence)
        .bind(sequence)
        .fetch_optional::<LedgerDetailChRow>()
        .await?;

    Ok(row.map(Into::into))
}

pub async fn fetch_transactions(
    client: &clickhouse::Client,
    ledger_sequence: i64,
    _closed_at: DateTime<Utc>,
    cursor: Option<&TsIdCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<LedgerTxRow>, clickhouse::error::Error> {
    let cursor_ts_ms = cursor.map(|c| c.ts.timestamp_millis());
    let cursor_application_order = cursor.map(|c| c.id);
    let (op, order) = direction_sql(direction);

    // Slim page query — base columns only, no correlated subqueries (CH 26.3
    // rejects those). `t.id` (hash surrogate) is selected as the aggregate
    // join key; the API cursor still keys on `application_order`.
    let sql = format!(
        "SELECT \
            t.id AS tx_surrogate, \
            lower(hex(t.hash)) AS hash, \
            t.ledger_sequence, \
            t.application_order, \
            nullIf(src.account_id, '') AS source_account, \
            t.fee_charged, \
            lower(hex(t.inner_tx_hash)) AS inner_tx_hash, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            l.closed_at AS created_at \
        FROM transactions t FINAL \
        INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
        LEFT JOIN accounts src FINAL ON src.id = t.source_id \
        WHERE t.ledger_sequence = ? \
          AND intDiv(t.ledger_sequence, 500000) = intDiv(?, 500000) \
          AND (isNull(?) OR (l.closed_at, toInt64(t.application_order)) {op} (fromUnixTimestamp64Milli(ifNull(?, 0)), ifNull(?, 0))) \
        ORDER BY l.closed_at {order}, t.application_order {order} \
        LIMIT ?",
    );

    let page = client
        .query(&sql)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .bind(cursor_ts_ms)
        .bind(cursor_ts_ms)
        .bind(cursor_application_order)
        .bind(limit)
        .fetch_all::<LedgerTxPageChRow>()
        .await?;

    // Second pass: aggregate operation_types + contract_ids for the page's
    // (ledger_sequence, transaction_id) keys (non-correlated; CH-26-safe),
    // then merge by the surrogate tx id.
    let keys: Vec<(i64, i64)> = page
        .iter()
        .map(|r| (r.ledger_sequence, r.tx_surrogate))
        .collect();
    let mut aggregates = ch::fetch_tx_list_aggregates(client, &keys).await?;
    Ok(page
        .into_iter()
        .map(|r| {
            let agg = aggregates.remove(&r.tx_surrogate).unwrap_or_default();
            r.into_ledger_tx_row(agg)
        })
        .collect())
}
