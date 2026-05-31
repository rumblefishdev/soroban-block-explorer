//! ClickHouse queries for the transactions endpoints.
//!
//! The public response shape intentionally mirrors the PostgreSQL path —
//! the frontend consumes the generated `@rumblefish/api-types`, so the CH
//! path maps `clickhouse::Row` structs back into the same `queries::*Row`
//! types the handler already knows. Notable CH-vs-PG divergences handled
//! here:
//!
//! - **No `transactions.created_at` on CH.** The API timestamp is the
//!   parent ledger `closed_at`, joined in from `ledgers` (ADR 0044 §5.2).
//! - **`transactions.id` is a deterministic hash surrogate**, not a
//!   `BIGSERIAL`. It is a stable, unique tie-break for the global list
//!   keyset `(ledger_sequence, id)` (canonical SQL 02), but it is NOT
//!   apply-order within a ledger — callers that need on-chain order use
//!   `application_order`.
//! - **`operations_appearances` has no `id` surrogate** (PR #175). The
//!   per-op `appearance_id` is the natural-key `application_order`
//!   (canonical SQL 03 statement C).
//! - **`soroban_events` is the full-payload table**, with no per-appearance
//!   fold-count column. The archive-unavailable fallback derives the
//!   `EventAppearanceItem.amount` as the per-contract event `count()` — a
//!   CH analogue of the PG `soroban_events_appearances.amount` fold count,
//!   not a token amount (same non-amount semantic as the PG column).
//!
//! ### List pagination + the partition-prune memory guard
//!
//! Canonical SQL 02 (PR #175 amendment) bounds every page to a single
//! `intDiv(ledger_sequence, 500000)` partition: a global
//! `transactions FINAL` scan over 20M+ rows otherwise blows the CH memory
//! limit. We reproduce that here — first page prunes to the latest
//! partition (`intDiv(max(sequence), 500000)`), subsequent pages prune to
//! the cursor's partition. The known cost is that backward pagination
//! across a 500k-ledger partition boundary stops early; this matches the
//! reference design and is acceptable for an explorer (deep cross-partition
//! cursor walks do not occur in the UI).
//!
//! `contract_ids` is populated on every statement, including the no-filter
//! path, for full parity with PG (team decision 2026-05-29). Two of the
//! three source subqueries (`soroban_invocations_appearances`,
//! `soroban_events`) are `ORDER BY (contract_id, …)`, so filtering them by
//! `transaction_id` is a partition scan rather than a PK seek — the exact
//! pattern canonical SQL 02 flagged as a memory risk. It is bounded here
//! because the inner `LIMIT` is applied before these correlated subqueries
//! run (≤ `limit + 1` driver rows), but the no-filter first-page path
//! still warrants an operator memory smoke before the prod flag flip.

use chrono::{DateTime, TimeZone, Utc};
use clickhouse::Row;
use domain::OperationType;
use serde::Deserialize;

use crate::common::cursor::{Direction, direction_sql};

use super::dto::TxListCursor;
use super::queries::{
    EventAppearanceRow, InvocationAppearanceRow, OpRow, ResolvedListParams, TxDetailRow, TxListRow,
};

// ---------------------------------------------------------------------------
// Row structs (positional decode — SELECT column order MUST match field order)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct TxListChRow {
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
    id: i64,
    created_at: i64,
}

impl From<TxListChRow> for TxListRow {
    fn from(row: TxListChRow) -> Self {
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
            operation_types: sorted_unique_labels(row.operation_type_codes),
            contract_ids: sorted_unique(row.contract_ids),
            created_at: millis_to_utc(row.created_at),
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct TxDetailChRow {
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
    created_at: i64,
    parse_error: bool,
}

impl From<TxDetailChRow> for TxDetailRow {
    fn from(row: TxDetailChRow) -> Self {
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
            created_at: millis_to_utc(row.created_at),
            parse_error: row.parse_error,
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct OpChRow {
    op_type: i16,
    source_account: Option<String>,
    destination_account: Option<String>,
    contract_id: Option<String>,
    asset_code: Option<String>,
    asset_issuer: Option<String>,
    pool_id: Option<String>,
    application_order: i16,
    ledger_sequence: i64,
    created_at: i64,
}

impl From<OpChRow> for OpRow {
    fn from(row: OpChRow) -> Self {
        Self {
            // CH `operations_appearances` dropped the BIGSERIAL surrogate
            // (PR #175); `application_order` is the natural per-op key and
            // the documented `appearance_id` replacement (canonical SQL 03
            // statement C). `OperationItem.appearance_id` is already
            // documented as an internal ordering artefact, so reusing the
            // apply-order index here is contract-safe.
            appearance_id: i64::from(row.application_order),
            type_name: operation_type_label(row.op_type),
            op_type: row.op_type,
            source_account: row.source_account.filter(|s| !s.is_empty()),
            destination_account: row.destination_account.filter(|s| !s.is_empty()),
            contract_id: row.contract_id.filter(|s| !s.is_empty()),
            asset_code: row.asset_code.filter(|s| !s.is_empty()),
            asset_issuer: row.asset_issuer.filter(|s| !s.is_empty()),
            pool_id: row.pool_id.filter(|s| !s.is_empty()),
            application_order: Some(row.application_order),
            ledger_sequence: row.ledger_sequence,
            created_at: millis_to_utc(row.created_at),
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct EventAppearanceChRow {
    contract_id: String,
    ledger_sequence: i64,
    amount: i64,
    created_at: i64,
}

impl From<EventAppearanceChRow> for EventAppearanceRow {
    fn from(row: EventAppearanceChRow) -> Self {
        Self {
            contract_id: row.contract_id,
            ledger_sequence: row.ledger_sequence,
            amount: row.amount,
            created_at: millis_to_utc(row.created_at),
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct InvocationAppearanceChRow {
    contract_id: String,
    caller_account: Option<String>,
    ledger_sequence: i64,
    amount: i32,
    created_at: i64,
}

impl From<InvocationAppearanceChRow> for InvocationAppearanceRow {
    fn from(row: InvocationAppearanceChRow) -> Self {
        Self {
            contract_id: row.contract_id,
            caller_account: row.caller_account.filter(|s| !s.is_empty()),
            ledger_sequence: row.ledger_sequence,
            amount: row.amount,
            created_at: millis_to_utc(row.created_at),
        }
    }
}

#[derive(Debug, Row, Deserialize)]
struct SurrogateIdRow {
    id: i64,
}

#[derive(Debug, Row, Deserialize)]
struct LedgerSeqRow {
    ledger_sequence: i64,
}

#[derive(Debug, Row, Deserialize)]
struct AccountIdRow {
    account_id: String,
}

// ---------------------------------------------------------------------------
// Shared projection fragments
// ---------------------------------------------------------------------------

/// Per-row projection shared by every list statement. Selects the slim list
/// columns plus the two correlated array aggregates (`operation_type_codes`,
/// `contract_ids`) and threads `t.id` + `l.closed_at` for the cursor and the
/// derived `created_at`. References only `t.*` / `l.*` columns — no binds.
const LIST_PROJECTION: &str = "\
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
    t.id, \
    l.closed_at AS created_at";

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

pub async fn fetch_list(
    client: &clickhouse::Client,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<TxListRow>, clickhouse::error::Error> {
    // Resolve StrKey filters to the CH surrogate ids up front. The writer's
    // `cityhash_102_128` surrogate is NOT bit-equivalent to CH's builtin
    // `cityHash64()` (schema header), so the id cannot be computed in SQL —
    // it is looked up against the `accounts` / `soroban_contracts` natural
    // keys. A filter that names a non-existent account/contract matches no
    // rows, so we short-circuit to an empty page.
    let source_id: Option<i64> = match params.source_account.as_deref() {
        Some(acct) => match resolve_account_surrogate(client, acct).await? {
            Some(id) => Some(id),
            None => return Ok(Vec::new()),
        },
        None => None,
    };
    let contract_surrogate: Option<i64> = match params.contract_id.as_deref() {
        Some(cid) => match resolve_contract_surrogate(client, cid).await? {
            Some(id) => Some(id),
            None => return Ok(Vec::new()),
        },
        None => None,
    };

    let (op, order) = direction_sql(direction);
    // Cursor keyset is `(ledger_sequence, id)` (canonical SQL 02). The CH
    // cursor variant carries `ledger_sequence` (partition key + primary sort)
    // and `tiebreak` (the `transactions.id` hash surrogate, within-ledger
    // tie-break). Both are present together or absent together, so the keyset
    // tuple never binds a NULL element. A `Pg`-variant cursor never reaches
    // here — `list_transactions` rejects a cross-datasource cursor with
    // `invalid_cursor` before dispatch — so the `_` arm only ever means
    // "first page".
    let (cursor_ledger, cursor_tiebreak): (Option<i64>, Option<i64>) = match params.cursor.as_ref()
    {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => (Some(*ledger_sequence), Some(*tiebreak)),
        _ => (None, None),
    };

    let rows = match (contract_surrogate, params.op_type) {
        // --- Statement B: contract filter (optionally + op_type) -----------
        (Some(cid), op_type_opt) => {
            let arm = |table: &str| {
                format!(
                    "SELECT ledger_sequence, transaction_id FROM {table} FINAL \
                     WHERE contract_id = ? \
                       AND intDiv(ledger_sequence, 500000) \
                           = ifNull(intDiv(?, 500000), (SELECT intDiv(max(sequence), 500000) FROM ledgers)) \
                       AND (? IS NULL OR (ledger_sequence, transaction_id) {op} (?, ?))"
                )
            };
            let sql = format!(
                "SELECT {LIST_PROJECTION} \
                 FROM ( \
                    SELECT DISTINCT ledger_sequence, transaction_id FROM ( \
                        {arm_ops} UNION DISTINCT {arm_inv} UNION DISTINCT {arm_evt} \
                    ) u \
                    ORDER BY ledger_sequence {order}, transaction_id {order} \
                    LIMIT ? \
                 ) m \
                 INNER JOIN transactions t FINAL \
                    ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence \
                 LEFT JOIN accounts src ON src.id = t.source_id \
                 INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
                 WHERE (? IS NULL OR t.source_id = ?) \
                   AND (? IS NULL OR ( \
                        SELECT count() FROM operations_appearances oa2 FINAL \
                        WHERE oa2.transaction_id = t.id \
                          AND oa2.ledger_sequence = t.ledger_sequence \
                          AND oa2.type = ? \
                          AND intDiv(oa2.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000) \
                   ) > 0) \
                 ORDER BY t.ledger_sequence {order}, t.id {order} \
                 LIMIT ?",
                arm_ops = arm("operations_appearances"),
                arm_inv = arm("soroban_invocations_appearances"),
                arm_evt = arm("soroban_events"),
            );

            let mut q = client.query(&sql);
            // Three UNION arms, identical bind tuple each:
            // (contract_id, cursor_ledger, cursor_ledger, cursor_ledger, tiebreak)
            for _ in 0..3 {
                q = q
                    .bind(cid)
                    .bind(cursor_ledger)
                    .bind(cursor_ledger)
                    .bind(cursor_ledger)
                    .bind(cursor_tiebreak);
            }
            q = q
                .bind(params.limit * 4) // over-fetch: a tx may hit up to all 3 arms
                .bind(source_id)
                .bind(source_id)
                .bind(op_type_opt)
                .bind(op_type_opt)
                .bind(params.limit + 1); // peek row drives next-page detection
            q.fetch_all::<TxListChRow>().await?
        }

        // --- Statement C: op_type filter only ------------------------------
        (None, Some(op_type)) => {
            let sql = format!(
                "SELECT {LIST_PROJECTION} \
                 FROM ( \
                    SELECT DISTINCT ledger_sequence, transaction_id \
                    FROM operations_appearances FINAL \
                    WHERE type = ? \
                      AND intDiv(ledger_sequence, 500000) \
                          = ifNull(intDiv(?, 500000), (SELECT intDiv(max(sequence), 500000) FROM ledgers)) \
                      AND (? IS NULL OR (ledger_sequence, transaction_id) {op} (?, ?)) \
                    ORDER BY ledger_sequence {order}, transaction_id {order} \
                    LIMIT ? \
                 ) m \
                 INNER JOIN transactions t FINAL \
                    ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence \
                 LEFT JOIN accounts src ON src.id = t.source_id \
                 INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
                 WHERE (? IS NULL OR t.source_id = ?) \
                 ORDER BY t.ledger_sequence {order}, t.id {order} \
                 LIMIT ?",
            );
            // No outer keyset re-check: the driver subquery already filtered
            // `(ledger_sequence, transaction_id) {op} (cursor)`, and the JOIN
            // binds `t.id = m.transaction_id` / `t.ledger_sequence =
            // m.ledger_sequence`, so every joined row already satisfies it.
            client
                .query(&sql)
                .bind(op_type)
                .bind(cursor_ledger)
                .bind(cursor_ledger)
                .bind(cursor_tiebreak)
                .bind(params.limit * 4)
                .bind(source_id)
                .bind(source_id)
                .bind(params.limit + 1) // peek row drives next-page detection
                .fetch_all::<TxListChRow>()
                .await?
        }

        // --- Statement A: no contract / op_type filter (default path) ------
        (None, None) => {
            let sql = format!(
                "SELECT {LIST_PROJECTION} \
                 FROM ( \
                    SELECT * FROM transactions FINAL \
                    WHERE intDiv(ledger_sequence, 500000) \
                          = ifNull(intDiv(?, 500000), (SELECT intDiv(max(sequence), 500000) FROM ledgers)) \
                      AND (? IS NULL OR (ledger_sequence, id) {op} (?, ?)) \
                      AND (? IS NULL OR source_id = ?) \
                    ORDER BY ledger_sequence {order}, id {order} \
                    LIMIT ? \
                 ) t \
                 LEFT JOIN accounts src ON src.id = t.source_id \
                 INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
                 ORDER BY t.ledger_sequence {order}, t.id {order}",
            );
            client
                .query(&sql)
                .bind(cursor_ledger)
                .bind(cursor_ledger)
                .bind(cursor_ledger)
                .bind(cursor_tiebreak)
                .bind(source_id)
                .bind(source_id)
                .bind(params.limit + 1)
                .fetch_all::<TxListChRow>()
                .await?
        }
    };

    Ok(rows.into_iter().map(Into::into).collect())
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

/// Resolve a transaction hash → parent `ledger_sequence`.
///
/// Reads `transaction_hash_index` directly (PK seek on `hash`), mirroring
/// the PG `lookup_hash_index`. Canonical SQL 03 uses the
/// `transaction_hash_dict` Dictionary as the O(1) hot path; that is a
/// CH-only optimization that can be layered on later without changing this
/// signature. `hash → ledger_sequence` is immutable, so no `FINAL` is
/// required on the ReplacingMergeTree index.
pub async fn lookup_hash_ledger(
    client: &clickhouse::Client,
    hash_hex: &str,
) -> Result<Option<i64>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT ledger_sequence FROM transaction_hash_index \
             WHERE hash = unhex(?) LIMIT 1",
        )
        .bind(hash_hex)
        .fetch_optional::<LedgerSeqRow>()
        .await?;
    Ok(row.map(|r| r.ledger_sequence))
}

pub async fn fetch_detail(
    client: &clickhouse::Client,
    hash_hex: &str,
    ledger_sequence: i64,
) -> Result<Option<TxDetailRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                t.id, \
                lower(hex(t.hash)) AS hash, \
                t.ledger_sequence, \
                t.application_order, \
                nullIf(src.account_id, '') AS source_account, \
                t.fee_charged, \
                lower(hex(t.inner_tx_hash)) AS inner_tx_hash, \
                t.successful, \
                t.operation_count, \
                t.has_soroban, \
                l.closed_at AS created_at, \
                t.parse_error \
             FROM transactions t FINAL \
             LEFT JOIN accounts src ON src.id = t.source_id \
             INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
             WHERE t.ledger_sequence = ? AND t.hash = unhex(?)",
        )
        .bind(ledger_sequence)
        .bind(hash_hex)
        .fetch_optional::<TxDetailChRow>()
        .await?;
    Ok(row.map(Into::into))
}

pub async fn fetch_operations(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<OpRow>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT \
                oa.type AS op_type, \
                nullIf(src.account_id, '') AS source_account, \
                nullIf(dst.account_id, '') AS destination_account, \
                nullIf(sc.contract_id, '') AS contract_id, \
                nullIf(oa.asset_code, '') AS asset_code, \
                nullIf(iss.account_id, '') AS asset_issuer, \
                lower(hex(oa.pool_id)) AS pool_id, \
                oa.application_order, \
                oa.ledger_sequence, \
                l.closed_at AS created_at \
             FROM operations_appearances oa FINAL \
             LEFT JOIN accounts          src FINAL ON src.id = oa.source_id \
             LEFT JOIN accounts          dst FINAL ON dst.id = oa.destination_id \
             LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = oa.contract_id \
             LEFT JOIN accounts          iss FINAL ON iss.id = oa.asset_issuer_id \
             INNER JOIN ledgers          l         ON l.sequence = oa.ledger_sequence \
             WHERE oa.transaction_id = ? \
               AND oa.ledger_sequence = ? \
               AND intDiv(oa.ledger_sequence, 500000) = intDiv(?, 500000) \
             ORDER BY oa.application_order",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<OpChRow>()
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn fetch_participants(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<String>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT a.account_id \
             FROM transaction_participants tp FINAL \
             JOIN accounts a FINAL ON a.id = tp.account_id \
             WHERE tp.transaction_id = ? \
               AND tp.ledger_sequence = ? \
               AND intDiv(tp.ledger_sequence, 500000) = intDiv(?, 500000) \
             ORDER BY a.account_id",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<AccountIdRow>()
        .await?;
    Ok(rows.into_iter().map(|r| r.account_id).collect())
}

pub async fn fetch_event_appearances(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<EventAppearanceRow>, clickhouse::error::Error> {
    // CH `soroban_events` is the full-payload table (no fold-count column).
    // The PG `EventAppearanceItem.amount` is a per-(contract, ledger)
    // appearance fold count; the CH analogue is the per-contract event
    // `count()` in this tx. Both are non-token "how many" counters, so the
    // wire shape and semantic match (`amount` is never a stroop value).
    let rows = client
        .query(
            "SELECT \
                sc.contract_id, \
                se.ledger_sequence, \
                toInt64(count()) AS amount, \
                any(l.closed_at) AS created_at \
             FROM soroban_events se FINAL \
             JOIN soroban_contracts sc FINAL ON sc.id = se.contract_id \
             JOIN ledgers l ON l.sequence = se.ledger_sequence \
             WHERE se.transaction_id = ? \
               AND se.ledger_sequence = ? \
               AND intDiv(se.ledger_sequence, 500000) = intDiv(?, 500000) \
             GROUP BY sc.contract_id, se.ledger_sequence \
             ORDER BY se.ledger_sequence, sc.contract_id",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<EventAppearanceChRow>()
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn fetch_invocation_appearances(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT \
                sc.contract_id, \
                nullIf(caller.account_id, '') AS caller_account, \
                sia.ledger_sequence, \
                sia.amount, \
                l.closed_at AS created_at \
             FROM soroban_invocations_appearances sia FINAL \
             JOIN soroban_contracts sc FINAL ON sc.id = sia.contract_id \
             LEFT JOIN accounts caller FINAL ON caller.id = sia.caller_id \
             INNER JOIN ledgers l ON l.sequence = sia.ledger_sequence \
             WHERE sia.transaction_id = ? \
               AND sia.ledger_sequence = ? \
               AND intDiv(sia.ledger_sequence, 500000) = intDiv(?, 500000) \
             ORDER BY sia.ledger_sequence, sc.contract_id",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<InvocationAppearanceChRow>()
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

// ---------------------------------------------------------------------------
// StrKey → surrogate-id resolution
// ---------------------------------------------------------------------------

async fn resolve_account_surrogate(
    client: &clickhouse::Client,
    account_strkey: &str,
) -> Result<Option<i64>, clickhouse::error::Error> {
    // `accounts.id` is deterministic across versions (cityhash of the
    // StrKey), so no FINAL is needed for the id lookup.
    let row = client
        .query("SELECT id FROM accounts WHERE account_id = ? LIMIT 1")
        .bind(account_strkey)
        .fetch_optional::<SurrogateIdRow>()
        .await?;
    Ok(row.map(|r| r.id))
}

async fn resolve_contract_surrogate(
    client: &clickhouse::Client,
    contract_strkey: &str,
) -> Result<Option<i64>, clickhouse::error::Error> {
    let row = client
        .query("SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1")
        .bind(contract_strkey)
        .fetch_optional::<SurrogateIdRow>()
        .await?;
    Ok(row.map(|r| r.id))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn millis_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("ClickHouse DateTime64(3, 'UTC') must decode to a valid UTC timestamp")
}

fn operation_type_label(code: i16) -> String {
    OperationType::try_from(code).map_or_else(|_| format!("UNKNOWN_{code}"), |op| op.to_string())
}

/// Map raw `operations_appearances.type` codes to canonical labels, then
/// sort + dedup for deterministic PG parity (`array_agg(DISTINCT … ORDER BY)`).
fn sorted_unique_labels(codes: Vec<i16>) -> Vec<String> {
    let mut labels: Vec<String> = codes.into_iter().map(operation_type_label).collect();
    labels.sort_unstable();
    labels.dedup();
    labels
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_type_label_known_and_unknown() {
        // A valid OperationType code maps to its SCREAMING_SNAKE label;
        // an out-of-range code degrades to a stable `UNKNOWN_<code>` tag
        // rather than panicking, so a forward-compatible op type from a
        // newer protocol still renders.
        assert_eq!(operation_type_label(0), "CREATE_ACCOUNT");
        assert_eq!(
            operation_type_label(i16::MAX),
            format!("UNKNOWN_{}", i16::MAX)
        );
    }

    #[test]
    fn sorted_unique_labels_dedups_and_orders() {
        // groupUniqArray returns codes in arbitrary order; the mapper must
        // produce the same deterministic, deduplicated, sorted label array
        // PG emits via `array_agg(DISTINCT … ORDER BY)`.
        let codes = vec![1, 0, 1]; // PAYMENT, CREATE_ACCOUNT, PAYMENT
        assert_eq!(
            sorted_unique_labels(codes),
            vec!["CREATE_ACCOUNT".to_string(), "PAYMENT".to_string()],
        );
    }

    #[test]
    fn sorted_unique_dedups_contract_ids() {
        let ids = vec!["C2".to_string(), "C1".to_string(), "C2".to_string()];
        assert_eq!(sorted_unique(ids), vec!["C1".to_string(), "C2".to_string()]);
    }

    #[test]
    fn list_row_maps_empty_sentinels_to_none() {
        // CH `nullIf(account_id, '')` and `hex(NULL)` already yield NULL →
        // None, but a defensive `.filter(!is_empty)` guards the empty-string
        // composite-PK sentinel. Verify None for empty, Some for populated,
        // and the millis → UTC timestamp decode.
        let row = TxListChRow {
            hash: "ab".repeat(32),
            ledger_sequence: 100,
            application_order: 2,
            source_account: Some(String::new()),
            fee_charged: 100,
            inner_tx_hash: None,
            successful: true,
            operation_count: 1,
            has_soroban: false,
            operation_type_codes: vec![1, 0],
            contract_ids: vec![],
            id: 999,
            created_at: 1_700_000_000_000,
        };
        let mapped: TxListRow = row.into();
        assert_eq!(mapped.source_account, None);
        assert_eq!(mapped.inner_tx_hash, None);
        assert_eq!(mapped.id, 999);
        assert_eq!(mapped.ledger_sequence, 100);
        assert_eq!(
            mapped.operation_types,
            vec!["CREATE_ACCOUNT".to_string(), "PAYMENT".to_string()],
        );
        assert_eq!(
            mapped.created_at,
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .unwrap(),
        );
    }

    #[test]
    fn op_row_uses_application_order_as_appearance_id() {
        // CH `operations_appearances` has no BIGSERIAL surrogate (PR #175);
        // `appearance_id` is the natural-key `application_order`, and the
        // op type code is mapped to its label in Rust (no `op_type_name`
        // SQL function on CH).
        let row = OpChRow {
            op_type: 1,
            source_account: Some("GSRC".to_string()),
            destination_account: Some(String::new()),
            contract_id: None,
            asset_code: Some(String::new()),
            asset_issuer: None,
            pool_id: None,
            application_order: 3,
            ledger_sequence: 100,
            created_at: 1_700_000_000_000,
        };
        let mapped: OpRow = row.into();
        assert_eq!(mapped.appearance_id, 3);
        assert_eq!(mapped.application_order, Some(3));
        assert_eq!(mapped.type_name, "PAYMENT");
        assert_eq!(mapped.source_account, Some("GSRC".to_string()));
        assert_eq!(mapped.destination_account, None); // empty sentinel → None
    }
}
