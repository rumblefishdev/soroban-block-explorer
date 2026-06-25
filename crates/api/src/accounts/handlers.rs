//! Axum handlers for the accounts endpoints.
//!
//! Both endpoints dispatch their DB reads through
//! `DataSource::for_module(Module::Accounts)` — PG (`sqlx`) or CH
//! (`clickhouse`) per the `API_DATASOURCE_ACCOUNTS` flag (task 0243). The
//! public response shape and the opaque cursor wire format are
//! datasource-agnostic; only the row fetches differ.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};

use crate::common::cache_control;
use crate::common::cursor::{self, Direction, SortOrder, TsIdCursor, parse_sort_order};
use crate::common::datasource::{DataSource, Module};
use crate::common::errors;
use crate::common::extractors::Pagination;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::path;
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;
use crate::transactions::dto::TxListCursor;

use super::dto::{
    AccountBalance, AccountDetailResponse, AccountListItem, AccountTransactionItem,
    AccountTxListParams, AccountsListParams,
};
use super::queries::{AccountBalanceRow, AccountHeaderRow, AccountTxRow};
use super::queries::{AccountListRow, AccountsListCursor, ResolvedListParams, fetch_list};
use super::{queries, queries_ch};

/// Unified per-call fetch error so the handlers dispatch between PG and CH
/// without leaking driver types up the call stack. Only `Display` is observed
/// (forwarded to the canonical `db_error` envelope + tracing).
#[derive(Debug, thiserror::Error)]
enum AcctFetchError {
    #[error("pg: {0}")]
    Pg(sqlx::Error),
    #[error("ch: {0}")]
    Ch(clickhouse::error::Error),
}
// ---------------------------------------------------------------------------
// GET /v1/accounts (list)
// ---------------------------------------------------------------------------

/// List accounts ordered by `last_seen_ledger` (the only indexed sort) —
/// newest-active first by default, oldest-first with `?order=asc`. The order
/// is sticky across pages; cursor pagination walks within it.
/// `filter[with_domain]` keeps only accounts that set a home_domain. No
/// address search — exact lookup is the global search's redirect path. Same
/// shape as the other list endpoints.
#[utoipa::path(
    get,
    path = "/accounts",
    tag = "accounts",
    params(
        ("limit"  = Option<u32>,    Query, description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor from a previous response."),
        AccountsListParams,
    ),
    responses(
        (status = 200, description = "Paginated account list",
         body = Paginated<AccountListItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_accounts(
    State(state): State<AppState>,
    pagination: Pagination<AccountsListCursor>,
    Query(params): Query<AccountsListParams>,
) -> Response {
    let sort = match parse_sort_order(params.order.as_deref()) {
        Ok(s) => s,
        Err(err) => return err.into_response(),
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let resolved = ResolvedListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        with_domain: params.filter_with_domain.unwrap_or(false),
    };

    let source = DataSource::for_module(Module::Accounts);
    let mut rows: Vec<AccountListRow> =
        match fetch_list_for_source(&state, source, &resolved, sort, direction).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(source = ?source, "DB error in list_accounts: {e}");
                return errors::internal_error(errors::DB_ERROR, "database error");
            }
        };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, r| {
            cursor::encode(
                &AccountsListCursor {
                    last_seen_ledger: r.last_seen_ledger,
                    id: r.id,
                },
                dir,
            )
        },
    );
    let data: Vec<AccountListItem> = rows.into_iter().map(map_item).collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

fn map_item(r: AccountListRow) -> AccountListItem {
    AccountListItem {
        account_id: r.account_id,
        xlm_balance: r.xlm_balance,
        last_seen_ledger: r.last_seen_ledger,
        first_seen_ledger: r.first_seen_ledger,
        home_domain: r.home_domain,
    }
}

// ---------------------------------------------------------------------------
// GET /v1/accounts/:account_id
// ---------------------------------------------------------------------------

/// Account detail — header from `accounts` + balances from
/// `account_balances_current` (canonical 06 statements A + B).
#[utoipa::path(
    get,
    path = "/accounts/{account_id}",
    tag = "accounts",
    params(
        ("account_id" = String, Path, description = "Stellar account StrKey (G…, 56 chars)"),
    ),
    responses(
        (status = 200, description = "Account detail with current balances",
         body = AccountDetailResponse),
        (status = 400, description = "Invalid account_id",        body = ErrorEnvelope),
        (status = 404, description = "Account not found",         body = ErrorEnvelope),
        (status = 500, description = "Internal server error",     body = ErrorEnvelope),
    ),
)]
pub async fn get_account(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&account_id, 'G', "account_id") {
        return resp;
    }

    let source = DataSource::for_module(Module::Accounts);

    let header = match fetch_account_for_source(&state, source, &account_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found(format!("account '{account_id}' not found")),
        Err(e) => {
            tracing::error!(source = ?source, "DB error fetching account {account_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let balances = match fetch_balances_for_source(&state, source, header.id).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| AccountBalance {
                asset_type_name: r.asset_type_name,
                asset_type: r.asset_type,
                asset_code: r.asset_code,
                asset_issuer: r.asset_issuer,
                balance: r.balance,
                last_updated_ledger: r.last_updated_ledger,
            })
            .collect(),
        Err(e) => {
            tracing::error!(source = ?source, "DB error fetching balances for {account_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let deleted = match fetch_deleted_for_source(&state, source, header.id, header.last_seen_ledger)
        .await
    {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(source = ?source, "DB error deriving deleted status for {account_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let body = AccountDetailResponse {
        account_id: header.account_id,
        sequence_number: header.sequence_number,
        balances,
        home_domain: header.home_domain,
        first_seen_ledger: header.first_seen_ledger,
        last_seen_ledger: header.last_seen_ledger,
        deleted,
    };

    let mut resp = Json(body).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

// ---------------------------------------------------------------------------
// GET /v1/accounts/:account_id/transactions
// ---------------------------------------------------------------------------

/// Paginated transactions involving the account (source or participant).
/// 404 when the StrKey is unknown — distinct from "indexed account, no
/// transactions yet" (matches assets/contracts sub-resource pattern).
#[utoipa::path(
    get,
    path = "/accounts/{account_id}/transactions",
    tag = "accounts",
    params(
        ("account_id" = String, Path, description = "Stellar account StrKey (G…, 56 chars)"),
        ("limit"  = Option<u32>,    Query, description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor from a previous response."),
        AccountTxListParams,
    ),
    responses(
        (status = 200, description = "Paginated transactions involving the account",
         body = Paginated<AccountTransactionItem>),
        (status = 400, description = "Invalid account_id / pagination", body = ErrorEnvelope),
        (status = 404, description = "Account not found",   body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn list_account_transactions(
    State(state): State<AppState>,
    pagination: Pagination<TxListCursor>,
    Path(account_id): Path<String>,
    Query(params): Query<AccountTxListParams>,
) -> Response {
    if let Err(resp) = path::strkey(&account_id, 'G', "account_id") {
        return resp;
    }

    let sort = match parse_sort_order(params.order.as_deref()) {
        Ok(s) => s,
        Err(err) => return err.into_response(),
    };

    let source = DataSource::for_module(Module::Accounts);

    // Reject a cursor minted for the other datasource (e.g. a PG cursor
    // replayed after a flag flip to CH). Its keyset is meaningless under the
    // active backend, so per ADR 0008 fail with `invalid_cursor` instead of
    // silently mis-paginating. A legacy/untagged cursor already fails decode
    // upstream in the extractor; this guards the decodes-but-wrong-intent case.
    if let Some(cursor) = &pagination.cursor
        && !cursor_matches_source(source, cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    let header = match fetch_account_for_source(&state, source, &account_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found(format!("account '{account_id}' not found")),
        Err(e) => {
            tracing::error!(source = ?source, "DB error resolving account {account_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();

    let mut rows = match fetch_account_tx_for_source(
        &state,
        source,
        header.id,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        sort,
        direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(source = ?source, "DB error in list_account_transactions: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, r| cursor::encode(&account_tx_cursor_for(source, r), dir),
    );
    let data: Vec<AccountTransactionItem> = rows
        .into_iter()
        .map(|r| AccountTransactionItem {
            hash: r.hash,
            ledger_sequence: r.ledger_sequence,
            application_order: r.application_order,
            source_account: r.source_account,
            fee_charged: r.fee_charged,
            successful: r.successful,
            operation_count: r.operation_count,
            has_soroban: r.has_soroban,
            operation_types: r.operation_types,
            created_at: r.created_at,
        })
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

// ---------------------------------------------------------------------------
// Per-source dispatch helpers
// ---------------------------------------------------------------------------

async fn fetch_list_for_source(
    state: &AppState,
    source: DataSource,
    params: &ResolvedListParams,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountListRow>, AcctFetchError> {
    match source {
        DataSource::Pg => fetch_list(&state.db, params, sort, direction)
            .await
            .map_err(AcctFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_list(state.ch(), params, sort, direction)
            .await
            .map_err(AcctFetchError::Ch),
    }
}

async fn fetch_account_for_source(
    state: &AppState,
    source: DataSource,
    account_strkey: &str,
) -> Result<Option<AccountHeaderRow>, AcctFetchError> {
    match source {
        DataSource::Pg => queries::fetch_account(&state.db, account_strkey)
            .await
            .map_err(AcctFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_account(state.ch(), account_strkey)
            .await
            .map_err(AcctFetchError::Ch),
    }
}

/// Derived `deleted` status (task 0324). CH-only: prod serves accounts from
/// CH, so the PG branch returns `false` rather than carrying a duplicate
/// derivation. See `queries_ch::fetch_deleted_status`.
async fn fetch_deleted_for_source(
    state: &AppState,
    source: DataSource,
    account_surrogate_id: i64,
    last_seen_ledger: i64,
) -> Result<bool, AcctFetchError> {
    match source {
        DataSource::Pg => Ok(false),
        DataSource::Ch => {
            queries_ch::fetch_deleted_status(state.ch(), account_surrogate_id, last_seen_ledger)
                .await
                .map_err(AcctFetchError::Ch)
        }
    }
}

async fn fetch_balances_for_source(
    state: &AppState,
    source: DataSource,
    account_id: i64,
) -> Result<Vec<AccountBalanceRow>, AcctFetchError> {
    match source {
        DataSource::Pg => queries::fetch_balances(&state.db, account_id)
            .await
            .map_err(AcctFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_balances(state.ch(), account_id)
            .await
            .map_err(AcctFetchError::Ch),
    }
}

async fn fetch_account_tx_for_source(
    state: &AppState,
    source: DataSource,
    account_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountTxRow>, AcctFetchError> {
    match source {
        DataSource::Pg => {
            // PG keys on `(created_at, transaction_id)`; extract that anchor
            // from the tagged cursor (the cross-source guard above guarantees
            // any present cursor is the `Pg` variant).
            let ts_cursor = cursor.and_then(|c| match c {
                TxListCursor::Pg { ts, id } => Some(TsIdCursor::new(*ts, *id)),
                TxListCursor::Ch { .. } => None,
            });
            queries::fetch_transactions(
                &state.db,
                account_id,
                limit,
                ts_cursor.as_ref(),
                sort,
                direction,
            )
            .await
            .map_err(AcctFetchError::Pg)
        }
        DataSource::Ch => {
            queries_ch::fetch_transactions(state.ch(), account_id, limit, cursor, sort, direction)
                .await
                .map_err(AcctFetchError::Ch)
        }
    }
}

/// Build the opaque account-transactions cursor for a boundary row. PG keys on
/// `(created_at, id)`; CH keys on `(ledger_sequence, id)` (the
/// `transaction_participants` / `transactions` keyset). Tagged with the active
/// datasource so a later request rejects a cursor minted for the other backend.
fn account_tx_cursor_for(source: DataSource, r: &AccountTxRow) -> TxListCursor {
    match source {
        DataSource::Pg => TxListCursor::Pg {
            ts: r.created_at,
            id: r.id,
        },
        DataSource::Ch => TxListCursor::Ch {
            ledger_sequence: r.ledger_sequence,
            tiebreak: r.id,
        },
    }
}

/// True when the decoded cursor was minted for the currently-active datasource
/// (ADR 0008 fail-clean on a cross-datasource replay).
fn cursor_matches_source(source: DataSource, cursor: &TxListCursor) -> bool {
    matches!(
        (source, cursor),
        (DataSource::Pg, TxListCursor::Pg { .. }) | (DataSource::Ch, TxListCursor::Ch { .. })
    )
}
