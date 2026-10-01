//! `GET /v1/liquidity-pools/{pool_id}/activity` — the handler.

#![allow(clippy::result_large_err)]

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};

use crate::common::cache_control;
use crate::common::cursor;
use crate::common::errors;
use crate::common::extractors::Pagination;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::path;
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;

use super::super::dto::{PoolActivityCursor, PoolActivityItem, PoolActivityParams, PoolEvent};
use super::super::queries;

/// The `allowed` list a `filter[event]` rejection returns. Derived from the
/// enum's own spellings rather than retyped, so it cannot advertise a value
/// `PoolEvent::from_param` would then refuse.
const ALLOWED_EVENTS: [&str; 3] = [
    PoolEvent::Trade.as_param(),
    PoolEvent::Deposit.as_param(),
    PoolEvent::Withdrawal.as_param(),
];

/// `GET /v1/liquidity-pools/{pool_id}/activity` — the pool's operations
/// (task 0491, issue #371), for classic and soroban pools alike (task 0374).
///
/// Supersedes `/transactions`, whose row was a transaction. That unit could
/// not carry an honest `Event` chip (a bundled deposit + trade collapsed to
/// one label), forced the Amount cell to stack figures that must not be
/// summed, and made a trades filter inexpressible — "trades only" has no
/// truthful answer for a transaction that deposits *and* trades. The old path
/// stays mounted until the frontend moves to this one (task 0491 step 3),
/// which is also when its handler, DTO and query go.
#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}/activity",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — a classic pool's SEP-23 strkey (`L…`) or a soroban pool's contract address (`C…`), 56 chars."),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
        ("filter[event]" = Option<String>, Query,
         description = "Restrict to `trade`, `deposit` or `withdrawal`."),
    ),
    responses(
        (status = 200, description = "Paginated pool activity, one row per operation (classic) or pool event (soroban)",
         body = Paginated<PoolActivityItem>),
        (status = 400, description = "Invalid pool_id, limit, cursor, or event", body = ErrorEnvelope),
        (status = 404, description = "Pool not found",  body = ErrorEnvelope),
        (status = 500, description = "Database error",  body = ErrorEnvelope),
    )
)]
pub async fn list_pool_activity(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    pagination: Pagination<PoolActivityCursor>,
    Query(params): Query<PoolActivityParams>,
) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };

    // The pool's kind and leg surrogates, which double as this path's
    // existence check: the driver pivots its rows' `asset_id` onto them, so
    // the read cannot run without them and a missing pool is one seek away
    // (task 0279's pairing, kept).
    let legs = queries::fetch_pool_asset_ids(&state.ch(), &pool_id_hex)
        .await
        .map_err(|e| e.to_string());
    let (pool_kind, leg_ids) = match legs {
        Ok(Some(found)) => found,
        Ok(None) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_asset_ids");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Validated here rather than by serde on the way in, so a bad value gets
    // this API's error envelope with the allowed list — same shape the chart's
    // `interval` returns.
    let event = match params.event.as_deref() {
        Some(s) => match PoolEvent::from_param(s) {
            Some(e) => Some(e),
            None => {
                return errors::bad_request_with_details(
                    errors::INVALID_FILTER,
                    "filter[event] must be one of: trade, deposit, withdrawal",
                    serde_json::json!({
                        "param": "filter[event]",
                        "received": s,
                        "allowed": ALLOWED_EVENTS,
                    }),
                );
            }
        },
        None => None,
    };

    // A classic pool's amounts are in `pool_operation_amounts`, a soroban
    // pool's in `pool_movements`; both come back as the same rows.
    let (ch, limit, cursor, direction) = (
        state.ch(),
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        pagination.direction,
    );
    let fetched = match pool_kind {
        domain::PoolKind::Classic => {
            queries::fetch_pool_activity(
                &ch,
                &pool_id_hex,
                &leg_ids,
                limit,
                cursor,
                direction,
                event,
            )
            .await
        }
        domain::PoolKind::Soroban => {
            queries::fetch_soroban_pool_activity(
                &ch,
                &pool_id_hex,
                &leg_ids,
                limit,
                cursor,
                direction,
                event,
            )
            .await
        }
    }
    .map_err(|e| e.to_string());
    let mut rows = match fetched {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_activity");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        pagination.direction,
        pagination.has_predecessor(),
        |dir, r| {
            cursor::encode(
                &PoolActivityCursor {
                    ledger_sequence: r.ledger_sequence,
                    application_order: r.application_order,
                    operation_index: r.operation_index,
                    event_index: r.event_index.unwrap_or(0),
                },
                dir,
            )
        },
    );
    let data: Vec<PoolActivityItem> = rows
        .into_iter()
        .map(|r| PoolActivityItem {
            transaction_hash: r.transaction_hash,
            ledger_sequence: r.ledger_sequence,
            operation_index: r.operation_index,
            event_index: r.event_index,
            event: r.event,
            amounts: r.amounts,
            source_account: r.source_account,
            pools_crossed: r.pools_crossed,
            created_at: r.created_at,
        })
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}
