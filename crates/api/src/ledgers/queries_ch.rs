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

use chrono::{DateTime, TimeZone, Utc};
use clickhouse::Row;
use domain::OperationType;
use serde::Deserialize;

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

#[derive(Debug, Row, Deserialize)]
struct LedgerTxChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    application_order: i16,
    source_account: Option<String>,
    fee_charged: i64,
    inner_tx_hash: Option<String>,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    operation_type_codes: Vec<i16>,
    contract_ids: Vec<String>,
    created_at: i64,
}

impl From<LedgerTxChRow> for LedgerTxRow {
    fn from(row: LedgerTxChRow) -> Self {
        let mut operation_types: Vec<String> = row
            .operation_type_codes
            .into_iter()
            .map(operation_type_label)
            .collect();
        operation_types.sort();
        operation_types.dedup();

        let mut contract_ids = row.contract_ids;
        contract_ids.sort();
        contract_ids.dedup();

        Self {
            id: row.id,
            hash: row.hash,
            ledger_sequence: row.ledger_sequence,
            application_order: row.application_order,
            source_account: row.source_account.filter(|s| !s.is_empty()),
            fee_charged: row.fee_charged,
            inner_tx_hash: row.inner_tx_hash.filter(|s| !s.is_empty()),
            successful: row.successful,
            operation_count: row.operation_count,
            has_soroban: row.has_soroban,
            operation_types,
            contract_ids,
            created_at: millis_to_utc(row.created_at),
        }
    }
}

pub async fn fetch_list(
    client: &clickhouse::Client,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    direction: Direction,
) -> Result<Vec<LedgerListItem>, clickhouse::error::Error> {
    let cursor_closed_at_ms = cursor.map(|c| c.ts.timestamp_millis());
    let cursor_sequence = cursor.map(|c| c.id);
    let (op, order) = direction_sql(direction);

    let sql = format!(
        "SELECT \
            l.sequence, \
            lower(hex(l.hash)) AS hash, \
            l.closed_at, \
            l.protocol_version, \
            l.transaction_count, \
            l.base_fee \
        FROM ledgers l \
        WHERE isNull(?) \
           OR (l.closed_at, l.sequence) {op} (fromUnixTimestamp64Milli(ifNull(?, 0)), ifNull(?, 0)) \
        ORDER BY l.closed_at {order}, l.sequence {order} \
        LIMIT ?",
    );

    let rows = client
        .query(&sql)
        .bind(cursor_closed_at_ms)
        .bind(cursor_closed_at_ms)
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

    let sql = format!(
        "SELECT \
            toInt64(t.application_order) AS id, \
            lower(hex(t.hash)) AS hash, \
            t.ledger_sequence, \
            t.application_order, \
            nullIf(src.account_id, '') AS source_account, \
            t.fee_charged, \
            lower(hex(t.inner_tx_hash)) AS inner_tx_hash, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            ( \
                SELECT groupUniqArray(oa.type) \
                FROM operations_appearances oa FINAL \
                WHERE oa.transaction_id = t.id \
                  AND oa.ledger_sequence = t.ledger_sequence \
                  AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000) \
            ) AS operation_type_codes, \
            arrayDistinct(arrayConcat( \
                (SELECT groupArray(sc.contract_id) \
                 FROM operations_appearances oa FINAL \
                 JOIN soroban_contracts sc FINAL ON sc.id = assumeNotNull(oa.contract_id) \
                 WHERE oa.transaction_id = t.id \
                   AND oa.ledger_sequence = t.ledger_sequence \
                   AND isNotNull(oa.contract_id) \
                   AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)), \
                (SELECT groupArray(sc.contract_id) \
                 FROM soroban_invocations_appearances sia FINAL \
                 JOIN soroban_contracts sc FINAL ON sc.id = sia.contract_id \
                 WHERE sia.transaction_id = t.id \
                   AND sia.ledger_sequence = t.ledger_sequence \
                   AND intDiv(sia.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)), \
                (SELECT groupArray(sc.contract_id) \
                 FROM soroban_events se FINAL \
                 JOIN soroban_contracts sc FINAL ON sc.id = se.contract_id \
                 WHERE se.transaction_id = t.id \
                   AND se.ledger_sequence = t.ledger_sequence \
                   AND intDiv(se.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)) \
            )) AS contract_ids, \
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

    let rows = client
        .query(&sql)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .bind(cursor_ts_ms)
        .bind(cursor_ts_ms)
        .bind(cursor_application_order)
        .bind(limit)
        .fetch_all::<LedgerTxChRow>()
        .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

fn millis_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("ClickHouse DateTime64(3, 'UTC') must decode to a valid UTC timestamp")
}

fn operation_type_label(code: i16) -> String {
    OperationType::try_from(code).map_or_else(|_| format!("UNKNOWN_{code}"), |op| op.to_string())
}
