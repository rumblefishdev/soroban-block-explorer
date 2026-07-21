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

use crate::common::ch::{self, millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, SortOrder, TsIdCursor, keyset_sql, keyset_sql_desc};
use crate::transactions::dto::{TransactionListItem, TransactionValue};

use super::dto::LedgerListItem;

// ---------------------------------------------------------------------------
// Internal query-result rows (not serialized; the handler maps these into the
// public response DTOs).
// ---------------------------------------------------------------------------

/// LATERAL-derived navigation pair. Kept separate from the public DTO
/// because the response type composes this with an embedded paginated
/// list (`transactions`) that does not come from a single SQL row.
#[derive(Debug)]
pub struct LedgerDetailRow {
    pub sequence: i64,
    pub hash: String,
    pub closed_at: DateTime<Utc>,
    pub protocol_version: i32,
    pub transaction_count: i32,
    pub base_fee: i64,
    pub prev_sequence: Option<i64>,
    pub next_sequence: Option<i64>,
}

/// DB-side projection of an embedded transaction row. Owned by the ledgers
/// domain (kept separate from `transactions::TxListRow` so the ledger module
/// does not couple to the transaction module's internal query types). Maps to
/// the shared wire `TransactionListItem`, which the ledger-detail response
/// intentionally embeds. `id` is the internal cursor tie-break, not on the DTO.
#[derive(Debug)]
pub struct LedgerTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub application_order: i16,
    /// `None` for Variant A `parse_error` transactions whose envelope was
    /// unavailable (lore-0209).
    pub source_account: Option<String>,
    pub fee_charged: i64,
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
    pub values: Vec<TransactionValue>,
    pub created_at: DateTime<Utc>,
}

impl From<LedgerTxRow> for TransactionListItem {
    fn from(row: LedgerTxRow) -> Self {
        Self {
            hash: row.hash,
            ledger_sequence: row.ledger_sequence,
            application_order: row.application_order,
            source_account: row.source_account,
            fee_charged: row.fee_charged,
            inner_tx_hash: row.inner_tx_hash,
            successful: row.successful,
            operation_count: row.operation_count,
            has_soroban: row.has_soroban,
            operation_types: row.operation_types,
            values: row.values,
            created_at: row.created_at,
        }
    }
}

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
/// `operation_types` is fetched separately via
/// [`ch::fetch_tx_list_aggregates`] and merged by the surrogate tx id
/// (CH 26.3 cannot compute it inline with a correlated subquery).
#[derive(Debug, Row, Deserialize)]
struct LedgerTxPageChRow {
    /// `transactions.id` hash surrogate — the aggregate join key. NOT the
    /// API cursor id (CH `id` is not apply-order; see `into_ledger_tx_row`).
    tx_surrogate: i64,
    hash: String,
    ledger_sequence: i64,
    application_order: i16,
    source_id: i64,
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
    fn into_ledger_tx_row(
        self,
        agg: ch::TxListAggregates,
        source_account: Option<String>,
    ) -> LedgerTxRow {
        LedgerTxRow {
            id: i64::from(self.application_order),
            hash: self.hash,
            ledger_sequence: self.ledger_sequence,
            application_order: self.application_order,
            source_account,
            fee_charged: self.fee_charged,
            inner_tx_hash: self.inner_tx_hash.filter(|s| !s.is_empty()),
            successful: self.successful,
            operation_count: self.operation_count,
            has_soroban: self.has_soroban,
            operation_types: agg.operation_types,
            values: agg.values.into_iter().map(TransactionValue::from).collect(),
            created_at: millis_to_utc(self.created_at),
        }
    }
}

pub async fn fetch_list(
    client: &clickhouse::Client,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<LedgerListItem>, clickhouse::error::Error> {
    let cursor_sequence = cursor.map(|c| c.id);
    let (op, order) = keyset_sql(sort, direction);

    // `ledgers` is ORDER BY `sequence`. Paginating by `closed_at` (as the PG
    // path does) forces a full ~12M-row scan + sort on every page (measured),
    // and this endpoint is polled, so that scan recurs and eats the
    // `read_rows` quota. `sequence` is monotonic with `closed_at` (a later
    // ledger closes later), so ordering + keying on `sequence` alone yields the
    // identical page in primary-key read-in-order — a handful of rows per page.
    // `sequence` is unique, so it needs no tie-break. The cursor still carries
    // `closed_at` for the wire `ts`, but the keyset no longer reads it.
    //
    // Dedup: `ledgers` is a ReplacingMergeTree and ~12.8M sequences carry 2
    // content-identical rows (22 carry 3) in unmerged parts. Un-deduped, the
    // keyset page returns each sequence twice — doubled rows, and (because the
    // frontend keys table rows by `sequence`) duplicate React keys that break
    // reconciliation and pile up orphaned rows on every re-sort.
    //
    // Deduped by OVER-FETCH + consecutive collapse in Rust, NOT `FINAL` and NOT
    // `LIMIT 1 BY` — both defeat `optimize_read_in_order` on this seek. Measured
    // on the polled first page (lore-0420):
    //
    //   over-fetch (this)   1,349,927 rows    82 MiB    45 ms
    //   LIMIT 1 BY          4,463,222 rows   272 MiB   105 ms
    //   FINAL              25,964,595 rows  1.55 GiB   370 ms   ← whole table
    //
    // Over-fetching costs NOTHING here: the read is granule-bound, so `LIMIT 60`
    // and `LIMIT 20` read the identical 1,349,927 rows. Same approach-B pattern
    // as `assets::dedup_consecutive` (task 0364).
    //
    // Correct because `ORDER BY sequence` IS the primary key, so a sequence's
    // physical versions are contiguous, and the projected columns are identical
    // across them — "keep first" is deterministic.
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
        .bind(limit * LEDGER_OVERFETCH)
        .fetch_all::<LedgerListRow>()
        .await?;

    Ok(dedup_consecutive(rows, limit as usize)
        .into_iter()
        .map(Into::into)
        .collect())
}

/// Over-fetch factor for [`fetch_list`]. Prod carries at most 3 physical rows
/// per `sequence` (12.8M carry 2, 22 carry 3), so ×3 fills a full page even in
/// the worst observed case. Free: the read is granule-bound, so a wider `LIMIT`
/// reads the same rows (see the note in `fetch_list`). If duplication ever
/// exceeds ×3 the page merely comes back short — the keyset cursor still
/// advances off the last row, so pagination stays correct, never looping.
const LEDGER_OVERFETCH: i64 = 3;

/// Collapse consecutive same-`sequence` rows from the over-fetched page and
/// truncate to `limit`.
///
/// **Requires `raw` to be ordered by `sequence`** — it only compares against the
/// previous row, so non-adjacent duplicates pass through (see
/// `unsorted_input_is_not_deduplicated`, which pins that contract).
///
/// Two separate guarantees back this, easy to conflate:
/// - duplicates are ADJACENT because the query says `ORDER BY sequence`;
/// - the read is CHEAP because `sequence` is the primary key, so ClickHouse
///   serves it read-in-order.
///
/// The primary key buys the cost, not the adjacency. The ordering itself cannot
/// quietly disappear: this page is keyset-paginated, so the next cursor is taken
/// from the LAST row — drop the `ORDER BY` and pagination breaks loudly (wrong
/// cursor, skipped pages) long before deduplication does. That is why a
/// `HashSet` would be the wrong trade here: it would keep collapsing duplicates
/// on unordered input and mask a page that is already broken.
///
/// "Keep first" is deterministic because a duplicate pair is byte-identical in
/// the projected columns.
fn dedup_consecutive(raw: Vec<LedgerListRow>, limit: usize) -> Vec<LedgerListRow> {
    let mut out: Vec<LedgerListRow> = Vec::with_capacity(limit.min(raw.len()));
    for r in raw {
        if out.last().is_none_or(|p| p.sequence != r.sequence) {
            out.push(r);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
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
    let (op, order) = keyset_sql_desc(direction);

    // Slim page query — base columns only, no correlated subqueries (CH 26.3
    // rejects those). `t.id` (hash surrogate) is selected as the aggregate
    // join key; the API cursor still keys on `application_order`.
    let sql = format!(
        "SELECT \
            t.id AS tx_surrogate, \
            lower(hex(t.hash)) AS hash, \
            t.ledger_sequence, \
            t.application_order, \
            t.source_id AS source_id, \
            t.fee_charged, \
            lower(hex(t.inner_tx_hash)) AS inner_tx_hash, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            l.closed_at AS created_at \
        FROM transactions t FINAL \
        INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
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

    // Second pass: aggregate operation_types for the page's (ledger_sequence,
    // transaction_id) keys (non-correlated; CH-26-safe), then merge by the
    // surrogate tx id.
    let keys: Vec<(i64, i64)> = page
        .iter()
        .map(|r| (r.ledger_sequence, r.tx_surrogate))
        .collect();
    // Resolve source StrKeys by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN … FINAL ON src.id = t.source_id` (task 0354).
    let accounts = resolve_accounts(client, page.iter().map(|r| r.source_id).collect()).await?;
    let mut aggregates = ch::fetch_tx_list_aggregates(client, &keys, true).await?;
    Ok(page
        .into_iter()
        .map(|r| {
            let agg = aggregates.remove(&r.tx_surrogate).unwrap_or_default();
            let source_account = accounts
                .get(&r.source_id)
                .cloned()
                .filter(|s| !s.is_empty());
            r.into_ledger_tx_row(agg, source_account)
        })
        .collect())
}

#[cfg(test)]
mod dedup_tests {
    use super::{LedgerListRow, dedup_consecutive};

    fn row(sequence: i64) -> LedgerListRow {
        LedgerListRow {
            sequence,
            hash: format!("h{sequence}"),
            closed_at: sequence * 1000,
            protocol_version: 27,
            transaction_count: 1,
            base_fee: 100,
        }
    }

    /// The lore-0420 bug: the RMT hands back each sequence 2–3× and the page
    /// rendered them all, which also collided the frontend's `sequence` row key.
    /// A full page must come back as DISTINCT sequences in the original order.
    #[test]
    fn collapses_duplicate_sequences_and_fills_the_page() {
        // Desc page as the RMT physically returns it: every sequence doubled,
        // one tripled — over-fetched at ×3 (limit 4 → 12 raw rows).
        let raw = [100, 100, 99, 99, 98, 98, 98, 97, 97, 96, 96, 95]
            .into_iter()
            .map(row)
            .collect();

        let out = dedup_consecutive(raw, 4);

        assert_eq!(
            out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
            vec![100, 99, 98, 97],
            "duplicates must collapse and the page must still be full"
        );
    }

    /// Under-fill is survivable: a short page is fine, but it must never emit a
    /// duplicate (that is what broke React reconciliation).
    #[test]
    fn never_emits_duplicates_even_when_underfilled() {
        let raw = [50, 50, 50, 49, 49, 49].into_iter().map(row).collect();

        let out = dedup_consecutive(raw, 4);

        assert_eq!(
            out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
            vec![50, 49]
        );
    }

    /// Pins the precondition rather than hiding it: this collapses ADJACENT
    /// duplicates only, so unordered input passes duplicates straight through.
    ///
    /// That is deliberate. Ordering is not an incidental detail of this page —
    /// the keyset cursor is read off the last row, so losing `ORDER BY sequence`
    /// corrupts pagination itself. A set-based dedup would keep this test green
    /// while the page silently returned rows in the wrong order under a wrong
    /// cursor; failing here is the cheaper outcome.
    #[test]
    fn unsorted_input_is_not_deduplicated() {
        // Same three sequences as the sorted case, interleaved.
        let raw = [100, 99, 100, 99, 98, 98].into_iter().map(row).collect();

        let out = dedup_consecutive(raw, 6);

        assert_eq!(
            out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
            vec![100, 99, 100, 99, 98],
            "only the adjacent 98/98 pair collapses — the interleaved duplicates \
             survive, because ordering is the caller's contract"
        );
    }
}
