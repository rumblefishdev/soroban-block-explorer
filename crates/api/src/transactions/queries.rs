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
//! - **`soroban_events` is the full-payload table** (one row per event). The
//!   archive-unavailable fallback groups per (contract, ledger) to emit one
//!   appearance row per contract — the same wire shape as the PG appearance
//!   index (which additionally carries a fold-count column, not surfaced).

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{millis_to_utc, operation_type_label, resolve_accounts, resolve_contracts};

use chrono::{DateTime, Utc};

use super::dto::TxListCursor;

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TxListRow {
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct TxDetailRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub application_order: i16,
    /// `None` for Variant A `parse_error` transactions whose envelope was
    /// unavailable (lore-0209).
    pub source_account: Option<String>,
    /// The source account's on-ledger `home_domain`, for the SEP-2 federated
    /// address the frontend resolves from it (task 0443, issue #363).
    pub source_account_home_domain: Option<String>,
    pub fee_charged: i64,
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub created_at: DateTime<Utc>,
    pub parse_error: bool,
}

#[derive(Debug)]
pub struct OpRow {
    pub appearance_id: i64,
    pub type_name: String,
    pub op_type: i16,
    pub source_account: Option<String>,
    pub destination_account: Option<String>,
    pub contract_id: Option<String>,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// Crossed liquidity pools, hex-encoded (full crossed-pool list from
    /// path-payment claim atoms, task 0261/0268).
    pub pool_ids: Vec<String>,
    /// 1-based per-tx apply position (task 0192). `None` for pre-task-0192
    /// rows; the caller falls back to `appearance_id` ordering.
    pub application_order: Option<i16>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct EventAppearanceRow {
    pub contract_id: String,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct InvocationAppearanceRow {
    pub contract_id: String,
    pub caller_account: Option<String>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

/// Resolved, validated `GET /v1/transactions` list params.
pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<TxListCursor>,
    pub source_account: Option<String>,
    pub contract_id: Option<String>,
    pub op_type: Option<i16>,
}

#[derive(Debug, Row, Deserialize)]
struct SurrogateIdRow {
    id: i64,
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct TxDetailRawRow {
    id: i64,
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
    parse_error: bool,
}

#[derive(Debug, Row, Deserialize)]
struct OpRawRow {
    op_type: i16,
    source_id: Option<i64>,
    destination_id: Option<i64>,
    contract_id: Option<i64>,
    asset_issuer_id: Option<i64>,
    asset_code: Option<String>,
    pool_ids: Vec<String>,
    application_order: i16,
    ledger_sequence: i64,
    created_at: i64,
}

#[derive(Debug, Row, Deserialize)]
struct EventAppearanceRawRow {
    id: i64,
    ledger_sequence: i64,
    created_at: i64,
}

#[derive(Debug, Row, Deserialize)]
struct InvocationAppearanceRawRow {
    contract_surrogate: i64,
    caller_id: Option<i64>,
    ledger_sequence: i64,
    created_at: i64,
}

pub async fn fetch_detail(
    client: &clickhouse::Client,
    hash_hex: &str,
    ledger_sequence: i64,
) -> Result<Option<TxDetailRow>, clickhouse::error::Error> {
    let raw = client
        .query(
            "SELECT \
                t.id AS id, \
                lower(hex(t.hash)) AS hash, \
                t.ledger_sequence, \
                t.application_order, \
                t.source_id, \
                t.fee_charged, \
                lower(hex(t.inner_tx_hash)) AS inner_tx_hash, \
                t.successful, \
                t.operation_count, \
                t.has_soroban, \
                l.closed_at AS created_at, \
                t.parse_error \
             FROM transactions t FINAL \
             INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
             WHERE t.ledger_sequence = ? \
               AND (t.hash = unhex(?) OR t.inner_tx_hash = unhex(?))",
        )
        .bind(ledger_sequence)
        .bind(hash_hex)
        .bind(hash_hex)
        .fetch_optional::<TxDetailRawRow>()
        .await?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let source = fetch_source_account(client, raw.source_id).await?;
    Ok(Some(TxDetailRow {
        id: raw.id,
        hash: raw.hash,
        ledger_sequence: raw.ledger_sequence,
        application_order: raw.application_order,
        source_account: source
            .as_ref()
            .map(|s| s.account_id.clone())
            .filter(|s| !s.is_empty()),
        source_account_home_domain: source.and_then(|s| s.home_domain).filter(|s| !s.is_empty()),
        fee_charged: raw.fee_charged,
        inner_tx_hash: raw.inner_tx_hash.filter(|s| !s.is_empty()),
        successful: raw.successful,
        operation_count: raw.operation_count,
        has_soroban: raw.has_soroban,
        created_at: millis_to_utc(raw.created_at),
        parse_error: raw.parse_error,
    }))
}

#[derive(Debug, Row, Deserialize)]
struct SourceAccountRow {
    account_id: String,
    home_domain: Option<String>,
}

/// Source account StrKey + its `home_domain`, in the one seek the detail path
/// already paid for.
///
/// Not `resolve_accounts`: that helper dedups ReplacingMergeTree versions with
/// `LIMIT 1 BY id`, which is exact only for columns that never change across
/// versions. `account_id` is such a column; `home_domain` is not — an account
/// can set, change or clear it — so it needs `argMax` over the table's version
/// column (`ReplacingMergeTree(last_seen_ledger)`), or an arbitrary older
/// domain could be served.
async fn fetch_source_account(
    client: &clickhouse::Client,
    source_id: i64,
) -> Result<Option<SourceAccountRow>, clickhouse::error::Error> {
    client
        .query(
            "SELECT any(account_id) AS account_id, \
                    argMax(home_domain, last_seen_ledger) AS home_domain \
             FROM accounts WHERE id = ? GROUP BY id",
        )
        .bind(source_id)
        .fetch_optional::<SourceAccountRow>()
        .await
}

pub async fn fetch_operations(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<OpRow>, clickhouse::error::Error> {
    let raw = client
        .query(
            "SELECT \
                oa.type AS op_type, \
                oa.source_id, \
                oa.destination_id, \
                oa.contract_id, \
                oa.asset_issuer_id, \
                nullIf(oa.asset_code, '') AS asset_code, \
                arrayMap(x -> lower(hex(x)), oa.pool_ids) AS pool_ids, \
                oa.application_order, \
                oa.ledger_sequence, \
                l.closed_at AS created_at \
             FROM operations_appearances oa FINAL \
             /* ledgers l FINAL: ledgers is a ReplacingMergeTree with unmerged \
                duplicate rows. This was correct only because `oa FINAL` \
                propagates FINAL into the join — an implicit CH behavior. Made \
                explicit so dropping `oa FINAL` can't silently double every op. \
                Cheap: the join pins a single sequence. lore-0420 */ \
             INNER JOIN ledgers l FINAL ON l.sequence = oa.ledger_sequence \
             WHERE oa.transaction_id = ? \
               AND oa.ledger_sequence = ? \
               AND intDiv(oa.ledger_sequence, 500000) = intDiv(?, 500000) \
             ORDER BY oa.application_order",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<OpRawRow>()
        .await?;

    let account_ids = raw
        .iter()
        .flat_map(|r| [r.source_id, r.destination_id, r.asset_issuer_id])
        .flatten()
        .collect();
    let contract_ids = raw.iter().filter_map(|r| r.contract_id).collect();
    // Both resolve off `raw` alone — one wave, not two (task 0446).
    let (accounts, contracts) = tokio::join!(
        resolve_accounts(client, account_ids),
        resolve_contracts(client, contract_ids),
    );
    let accounts = accounts?;
    let contracts = contracts?;

    Ok(raw
        .into_iter()
        .map(|r| OpRow {
            // CH `operations_appearances` dropped the BIGSERIAL surrogate
            // (PR #175); `application_order` is the natural per-op key.
            appearance_id: i64::from(r.application_order),
            type_name: operation_type_label(r.op_type),
            op_type: r.op_type,
            source_account: r
                .source_id
                .and_then(|id| accounts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            destination_account: r
                .destination_id
                .and_then(|id| accounts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            contract_id: r
                .contract_id
                .and_then(|id| contracts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            asset_code: r.asset_code.filter(|s| !s.is_empty()),
            asset_issuer: r
                .asset_issuer_id
                .and_then(|id| accounts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            pool_ids: r.pool_ids,
            application_order: Some(r.application_order),
            ledger_sequence: r.ledger_sequence,
            created_at: millis_to_utc(r.created_at),
        })
        .collect())
}

/// The transaction's participants, located by its position (task 0575).
pub async fn fetch_participants(
    client: &clickhouse::Client,
    ledger_sequence: i64,
    application_order: i16,
) -> Result<Vec<String>, clickhouse::error::Error> {
    let raw = client
        .query(
            "SELECT tp.account_id AS id \
             FROM transaction_participants tp FINAL \
             WHERE tp.ledger_sequence = ? \
               AND tp.application_order = ? \
               AND intDiv(tp.ledger_sequence, 500000) = intDiv(?, 500000)",
        )
        .bind(ledger_sequence)
        .bind(application_order)
        .bind(ledger_sequence)
        .fetch_all::<SurrogateIdRow>()
        .await?;
    let accounts = resolve_accounts(client, raw.iter().map(|r| r.id).collect()).await?;
    // INNER JOIN semantics: drop participants whose account row is absent.
    let mut out: Vec<String> = raw
        .into_iter()
        .filter_map(|r| accounts.get(&r.id).cloned())
        .collect();
    out.sort();
    Ok(out)
}

/// One row per contract with an event in the transaction. Binds
/// `ledger_sequence`, `application_order`, `ledger_sequence`.
///
/// No `FINAL`: the `GROUP BY` already collapses duplicate rows. The filter is
/// the transaction's position — a fee refund's rpc id carries a sentinel, not
/// the transaction, so `application_order` is what names it (ADR 0059).
fn event_appearances_sql() -> &'static str {
    "SELECT \
        se.contract_id AS id, \
        se.ledger_sequence, \
        any(l.closed_at) AS created_at \
     FROM soroban_events se \
     JOIN ledgers l ON l.sequence = se.ledger_sequence \
     WHERE se.ledger_sequence = ? \
       AND se.application_order = ? \
       AND intDiv(se.ledger_sequence, 500000) = intDiv(?, 500000) \
     GROUP BY se.contract_id, se.ledger_sequence"
}

pub async fn fetch_event_appearances(
    client: &clickhouse::Client,
    ledger_sequence: i64,
    application_order: i16,
) -> Result<Vec<EventAppearanceRow>, clickhouse::error::Error> {
    // CH `soroban_events` is the full-payload table (one row per event). We
    // group per (contract, ledger) to produce one appearance row per contract
    // in this tx — the same wire shape as the PG appearance index.
    let raw = client
        .query(event_appearances_sql())
        .bind(ledger_sequence)
        .bind(application_order)
        .bind(ledger_sequence)
        .fetch_all::<EventAppearanceRawRow>()
        .await?;
    let contracts = resolve_contracts(client, raw.iter().map(|r| r.id).collect()).await?;
    let mut out: Vec<EventAppearanceRow> = raw
        .into_iter()
        .map(|r| EventAppearanceRow {
            contract_id: contracts.get(&r.id).cloned().unwrap_or_default(),
            ledger_sequence: r.ledger_sequence,
            created_at: millis_to_utc(r.created_at),
        })
        .collect();
    // Matches the old `ORDER BY se.ledger_sequence, contract_id` (resolved StrKey).
    out.sort_by(|a, b| {
        (a.ledger_sequence, &a.contract_id).cmp(&(b.ledger_sequence, &b.contract_id))
    });
    Ok(out)
}

pub async fn fetch_invocation_appearances(
    client: &clickhouse::Client,
    transaction_id: i64,
    ledger_sequence: i64,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    let raw = client
        .query(
            "SELECT \
                sia.contract_id AS contract_surrogate, \
                sia.caller_id, \
                sia.ledger_sequence, \
                l.closed_at AS created_at \
             FROM soroban_invocations_appearances sia FINAL \
             /* ledgers l FINAL: defensive dedup — see fetch_operations. Was \
                correct only via `sia FINAL` propagating into the join; made \
                explicit. Single-sequence pin, so cheap. lore-0420 */ \
             INNER JOIN ledgers l FINAL ON l.sequence = sia.ledger_sequence \
             WHERE sia.transaction_id = ? \
               AND sia.ledger_sequence = ? \
               AND intDiv(sia.ledger_sequence, 500000) = intDiv(?, 500000)",
        )
        .bind(transaction_id)
        .bind(ledger_sequence)
        .bind(ledger_sequence)
        .fetch_all::<InvocationAppearanceRawRow>()
        .await?;
    // Both resolve off `raw` alone — one wave, not two (task 0446).
    let (contracts, accounts) = tokio::join!(
        resolve_contracts(client, raw.iter().map(|r| r.contract_surrogate).collect()),
        resolve_accounts(client, raw.iter().filter_map(|r| r.caller_id).collect()),
    );
    let contracts = contracts?;
    let accounts = accounts?;
    let mut out: Vec<InvocationAppearanceRow> = raw
        .into_iter()
        .map(|r| InvocationAppearanceRow {
            contract_id: contracts
                .get(&r.contract_surrogate)
                .cloned()
                .unwrap_or_default(),
            caller_account: r
                .caller_id
                .and_then(|id| accounts.get(&id).cloned())
                .filter(|s| !s.is_empty()),
            ledger_sequence: r.ledger_sequence,
            created_at: millis_to_utc(r.created_at),
        })
        .collect();
    // Matches the old `ORDER BY sia.ledger_sequence, sc.contract_id` (resolved StrKey).
    out.sort_by(|a, b| {
        (a.ledger_sequence, &a.contract_id).cmp(&(b.ledger_sequence, &b.contract_id))
    });
    Ok(out)
}

mod hash_lookup;
mod list_transactions;
pub use hash_lookup::lookup_hash_ledgers;
pub use list_transactions::fetch_list;

#[cfg(test)]
mod tests;
