//! Axum handlers for the ledgers endpoints.
//!
//! Both endpoints are pure DB-only — list reads `ledgers` directly,
//! detail runs a header query plus a partition-pruned read of the
//! `transactions` partition for the embedded transactions[]. No archive
//! XDR fetch on either endpoint: list rows do not carry memo / heavy
//! fields (those live on the transaction detail endpoint instead).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::common::cache_control;
use crate::common::cursor::{Direction, SortOrder, TsIdCursor, parse_sort_order};
use crate::common::datasource::{DataSource, Module};
use crate::common::errors;
use crate::common::extractors::Pagination;
use crate::common::pagination::{finalize_ts_id_page, into_envelope};
use crate::common::path;
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;
use crate::transactions::dto::TransactionListItem;

use super::dto::{LedgerDetailResponse, LedgerListItem};
use super::queries::LedgerTxRow;
use super::{queries, queries_ch};

/// Base sort order for `GET /v1/ledgers` — a sticky query param the
/// client re-sends on every page. `order=asc|desc` sets the base sort for all
/// pages, and `fetch_list` receives this persistent sort parameter. The
/// cursor only controls the navigation direction (i.e., which page is fetched
/// next) rather than overriding order. The client should re-send `order` with
/// each paginated request alongside the `cursor`.
#[derive(Debug, Deserialize)]
pub struct LedgersListQuery {
    #[serde(default)]
    order: Option<String>,
}

// ---------------------------------------------------------------------------
// GET /v1/ledgers
// ---------------------------------------------------------------------------

/// List ledgers ordered by `(closed_at, sequence)` — newest-first by
/// default, oldest-first with `?order=asc`. The order is sticky across
/// pages; cursor pagination walks forward/back within the chosen order.
#[utoipa::path(
    get,
    path = "/ledgers",
    tag = "ledgers",
    params(
        ("limit"  = Option<u32>,    Query, description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor from a previous response."),
        ("order"  = Option<String>, Query,
         description = "Base sort order: `asc` = oldest→newest, `desc` (default) = newest→oldest. Sticky — re-send it on every page alongside `cursor`. Reset to the first page (drop `cursor`) when changing it."),
    ),
    responses(
        (status = 200, description = "Paginated ledger list",
         body = Paginated<LedgerListItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_ledgers(
    State(state): State<AppState>,
    pagination: Pagination<TsIdCursor>,
    Query(order_query): Query<LedgersListQuery>,
) -> Response {
    let source = DataSource::for_module(Module::Ledgers);
    // `?order=` sets the persistent base sort, which `fetch_list` receives.
    // The cursor only controls navigation direction rather than overriding order.
    // The client resets to page 1 when toggling order, and re-sends the order param
    // with each subsequent page request.
    let sort = match parse_sort_order(order_query.order.as_deref()) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    // Fetch limit+1 rows — the extra peek row drives continuation
    // detection in `finalize_ts_id_page` below. `sort` picks the base
    // order; `direction` (from the cursor) walks forward/back within it.
    let mut rows: Vec<LedgerListItem> = match fetch_list_for_source(
        &state,
        source,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        sort,
        pagination.direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(source = ?source, "DB error in list_ledgers: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Cursor maps closed_at → ts, sequence → id. Field names are opaque
    // to the client (cursor wire format is base64(JSON), per ADR 0008).
    // `Prev` rows are reversed in finalize so presentation matches `sort`.
    let page = finalize_ts_id_page(
        &mut rows,
        pagination.limit,
        pagination.direction,
        pagination.has_predecessor(),
        |r| r.closed_at,
        |r| r.sequence,
    );

    let mut resp = Json(into_envelope(rows, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

// ---------------------------------------------------------------------------
// GET /v1/ledgers/:sequence
// ---------------------------------------------------------------------------

/// Get ledger detail by sequence — header + prev/next navigation +
/// embedded paginated transactions.
///
/// Two phases, both DB-only:
///
/// 1. **DB header.** Resolve `:sequence` against `ledgers` + LATERAL
///    prev/next computed via `sequence` comparisons on the `ledgers`
///    PK (index-only scan, no heap fetch). 404 on miss.
/// 2. **DB transactions.** Keyset-paginated read of the `transactions`
///    partition with full equality partition prune
///    (`created_at = $closed_at`).
///
/// The detail endpoint reuses the standard `?limit=` / `?cursor=` query
/// parameters to drive embedded transactions pagination. Detail itself
/// is a single resource, so no naming collision arises — the params
/// page the embedded list directly.
#[utoipa::path(
    get,
    path = "/ledgers/{sequence}",
    tag = "ledgers",
    params(
        ("sequence" = i64,            Path,  description = "Ledger sequence number"),
        ("limit"    = Option<u32>,    Query, description = "Embedded transactions page size (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor"   = Option<String>, Query, description = "Embedded transactions opaque pagination cursor."),
    ),
    responses(
        (status = 200, description = "Ledger detail with embedded transactions",
         body = LedgerDetailResponse),
        (status = 400, description = "Invalid sequence format or pagination parameter", body = ErrorEnvelope),
        (status = 404, description = "Ledger not found",        body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn get_ledger(
    State(state): State<AppState>,
    Path(sequence_raw): Path<String>,
    pagination: Pagination<TsIdCursor>,
) -> Response {
    let source = DataSource::for_module(Module::Ledgers);
    // Path param shape-validate via the canonical helper from task 0044
    // (PR #132). Rejects non-numeric / negative / zero / >u32::MAX inputs
    // with `code = "invalid_sequence"` and `details.{param,received}`.
    let sequence: i64 = match path::sequence(&sequence_raw) {
        Ok(n) => i64::from(n),
        Err(resp) => return resp,
    };

    // Phase 1 — DB header.
    let header_row = match fetch_by_sequence_for_source(&state, source, sequence).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found(format!("ledger with sequence {sequence} not found")),
        Err(e) => {
            tracing::error!(source = ?source, "DB error in get_ledger header: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Phase 2 — DB embedded transactions, keyset-paginated by
    // `?limit=` / `?cursor=` query params validated above.
    let mut tx_rows: Vec<LedgerTxRow> = match fetch_transactions_for_source(
        &state,
        source,
        header_row.sequence,
        header_row.closed_at,
        pagination.cursor.as_ref(),
        pagination.fetch_limit(),
        pagination.direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(source = ?source, "DB error in get_ledger transactions: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let tx_page = finalize_ts_id_page(
        &mut tx_rows,
        pagination.limit,
        pagination.direction,
        pagination.has_predecessor(),
        |r| r.created_at,
        |r| r.id,
    );

    let tx_data: Vec<TransactionListItem> = tx_rows.into_iter().map(Into::into).collect();

    let body = LedgerDetailResponse {
        sequence: header_row.sequence,
        hash: header_row.hash,
        closed_at: header_row.closed_at,
        protocol_version: header_row.protocol_version,
        transaction_count: header_row.transaction_count,
        base_fee: header_row.base_fee,
        prev_sequence: header_row.prev_sequence,
        next_sequence: header_row.next_sequence,
        transactions: into_envelope(tx_data, tx_page),
    };

    // `next_sequence` is null only at the chain head; older ledgers are
    // immutable per Stellar consensus and safe to cache long.
    let cache_value = if body.next_sequence.is_none() {
        cache_control::SHORT
    } else {
        cache_control::LONG
    };

    let mut resp = Json(body).into_response();
    cache_control::attach(&mut resp, cache_value);
    resp
}

async fn fetch_list_for_source(
    state: &AppState,
    source: DataSource,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<LedgerListItem>, LedgerFetchError> {
    match source {
        DataSource::Pg => queries::fetch_list(&state.db, limit, cursor, sort, direction)
            .await
            .map_err(LedgerFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_list(state.ch(), limit, cursor, sort, direction)
            .await
            .map_err(LedgerFetchError::Ch),
    }
}

async fn fetch_by_sequence_for_source(
    state: &AppState,
    source: DataSource,
    sequence: i64,
) -> Result<Option<queries::LedgerDetailRow>, LedgerFetchError> {
    match source {
        DataSource::Pg => queries::fetch_by_sequence(&state.db, sequence)
            .await
            .map_err(LedgerFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_by_sequence(state.ch(), sequence)
            .await
            .map_err(LedgerFetchError::Ch),
    }
}

async fn fetch_transactions_for_source(
    state: &AppState,
    source: DataSource,
    ledger_sequence: i64,
    closed_at: chrono::DateTime<chrono::Utc>,
    cursor: Option<&TsIdCursor>,
    limit: i64,
    direction: crate::common::cursor::Direction,
) -> Result<Vec<LedgerTxRow>, LedgerFetchError> {
    match source {
        DataSource::Pg => queries::fetch_transactions(
            &state.db,
            ledger_sequence,
            closed_at,
            cursor,
            limit,
            direction,
        )
        .await
        .map_err(LedgerFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_transactions(
            state.ch(),
            ledger_sequence,
            closed_at,
            cursor,
            limit,
            direction,
        )
        .await
        .map_err(LedgerFetchError::Ch),
    }
}

#[derive(Debug, thiserror::Error)]
enum LedgerFetchError {
    #[error("pg: {0}")]
    Pg(sqlx::Error),
    #[error("ch: {0}")]
    Ch(clickhouse::error::Error),
}
