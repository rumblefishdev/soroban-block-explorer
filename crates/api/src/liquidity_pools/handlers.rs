//! Handlers for the liquidity-pool endpoints (participants from task 0126;
//! list / detail / activity / chart from tasks 0052 and 0491).

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
use crate::common::pool_asset_codes::normalize_asset_codes;
use crate::common::strkey::{pool_id_from_text, pool_id_hex_to_strkey};
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;

use super::dto::{
    ParticipantItem, PoolAssetLeg, PoolItem, PoolListCursor, PoolListParams, SharesCursor,
};
use super::queries::{self, PoolLegRow, PoolRow, ResolvedPoolListParams};

mod get_pool_chart;
mod list_pool_activity;
pub use get_pool_chart::*;
pub use list_pool_activity::*;

#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}/participants",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — a classic pool's SEP-23 strkey (`L…`) or a soroban pool's contract address (`C…`), 56 chars."),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated participants list",
         body = Paginated<ParticipantItem>),
        (status = 400, description = "Invalid pool_id, limit, or cursor; or `not_indexed`: \
         a soroban pool whose providers are not readable (no share token — a concentrated \
         pool — or a token that publishes no decimals)", body = ErrorEnvelope),
        (status = 404, description = "Pool not found",  body = ErrorEnvelope),
        (status = 500, description = "Database error",  body = ErrorEnvelope),
    )
)]
pub async fn list_participants(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    pagination: Pagination<SharesCursor>,
) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };

    // Fetch limit + 1 so `finalize_page` can detect a next page without
    // a separate count query.
    let fetch_limit = pagination.fetch_limit();
    let has_predecessor = pagination.has_predecessor();
    let direction = pagination.direction;

    // 404 vs 200-empty disambiguation: a missing pool gets 404 so the
    // frontend can route to a "pool not found" page. An existing pool
    // with no current participants returns 200 with `data: []`.
    //
    // Both reads derive everything from the path — the page never consumes the
    // existence answer — so they go out together (task 0446). `exists` is still
    // what decides the 404 and is still checked first, so responses are
    // unchanged; the cost is one wasted page read when the pool is missing.
    //
    // A `C…` id is a soroban pool: its providers are the holders of its share
    // token, not `lp_positions` rows.
    let ch = state.ch();
    let soroban = pool_id.starts_with('C');
    let (exists, fetched) = tokio::join!(queries::pool_exists(&ch, &pool_id_hex), async {
        if soroban {
            queries::fetch_soroban_participants(
                &ch,
                &pool_id_hex,
                pagination.cursor.as_ref(),
                fetch_limit,
                direction,
            )
            .await
        } else {
            queries::fetch_participants(
                &ch,
                &pool_id_hex,
                pagination.cursor.as_ref(),
                fetch_limit,
                direction,
            )
            .await
            .map(Some)
        }
    },);
    match exists.map_err(|e| e.to_string()) {
        Ok(true) => {}
        Ok(false) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in pool_exists");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    }

    let mut rows = match fetched.map_err(|e| e.to_string()) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return errors::bad_request(
                errors::NOT_INDEXED,
                "this pool's providers are not indexed: it has no share token \
                 (a concentrated pool keeps positions), or the token publishes no \
                 decimals",
            );
        }
        Err(e) => {
            tracing::error!(pool_id = %pool_id, soroban, error = %e, "DB error in the participants read");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Cursor builder gets the kept tail / head row directly — both the
    // wire `shares` (NUMERIC string) and the internal
    // `account_id_surrogate` BIGINT travel inside the opaque payload,
    // never on the wire.
    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, last| {
            cursor::encode(
                &SharesCursor {
                    shares: last.cursor_shares.clone(),
                    account_id: last.account_id_surrogate,
                },
                dir,
            )
        },
    );

    let data: Vec<ParticipantItem> = rows
        .into_iter()
        .map(|r| ParticipantItem {
            account: r.account,
            shares: r.shares,
            share_percentage: r.share_percentage,
            first_deposit_ledger: r.first_deposit_ledger,
            last_updated_ledger: r.last_updated_ledger,
        })
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

// ---------------------------------------------------------------------------
// List / Detail / Activity / Chart (tasks 0052, 0491)
// ---------------------------------------------------------------------------

// `normalize_asset_codes` used to live here. It moved to
// `common::pool_asset_codes` when global search adopted the same matching rule
// (task 0470) — the needle split and the WHERE clause it feeds have to agree,
// so they now sit in one module together. The `splitn(2)` bound, the
// empty-needle drop and the pair semantics are documented there.

fn map_leg(leg: PoolLegRow) -> PoolAssetLeg {
    PoolAssetLeg {
        asset_type_name: domain::AssetFamily::try_from(leg.family)
            .ok()
            .map(|f| f.as_str().to_string()),
        asset_code: leg.asset_code,
        issuer: leg.issuer,
        contract_id: leg.contract_id,
        symbol: leg.symbol,
        icon_url: leg.icon_url,
        reserve: leg.reserve,
    }
}

fn map_pool_item(row: PoolRow) -> PoolItem {
    PoolItem {
        // The same 32 bytes are an `L…` strkey for a classic pool and a `C…`
        // address for a soroban one, and the wrong form is well-formed rather
        // than an error — so the encoding follows the kind.
        pool_id: pool_id_hex_to_strkey(&row.pool_id_hex, row.pool_kind),
        pool_kind: row.pool_kind,
        protocol: super::protocol_labels::protocol_of(row.deployment_id).map(str::to_string),
        legs: row.legs.into_iter().map(map_leg).collect(),
        fee_bps: row.fee_bps,
        fee_percent: row.fee_percent,
        created_at_ledger: row.created_at_ledger,
        participant_count: row.participant_count,
        latest_snapshot_ledger: row.latest_snapshot_ledger,
        total_shares: row.total_shares,
        tvl: row.tvl,
        volume: row.volume,
        fee_revenue: row.fee_revenue,
        latest_snapshot_at: row.latest_snapshot_at,
    }
}

#[utoipa::path(
    get,
    path = "/liquidity-pools",
    tag = "liquidity-pools",
    params(
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
        PoolListParams,
    ),
    responses(
        (status = 200, description = "Paginated liquidity-pool list",
         body = Paginated<PoolItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_pools(
    State(state): State<AppState>,
    pagination: Pagination<PoolListCursor>,
    Query(params): Query<PoolListParams>,
) -> Response {
    // `filter[min_tvl]` is REJECTED, not ignored and not silently empty.
    //
    // Its SQL pre-filter reads `liquidity_pool_snapshots.tvl`, a column task
    // 0199 established is never written (USD is computed at read, ADR 0053),
    // so the predicate matched nothing and the endpoint answered "no pools"
    // — while the same response now carries real per-row USD `tvl`. A filter
    // that contradicts the rows it filters is worse than an absent one, so
    // callers get a 400 that says why rather than a plausible empty page.
    //
    // Restoring it needs TVL for ALL pools per request (it changes page
    // membership, so it cannot ride the per-page price lookup) — i.e. the
    // prices-side identity-keyed materialization. Until then this stays a
    // 400 and `ResolvedPoolListParams::min_tvl` stays `None`.
    if let Some(min) = params.filter_min_tvl.as_deref() {
        return errors::bad_request_with_details(
            errors::INVALID_FILTER,
            "filter[min_tvl] is not supported: pool TVL is computed at read \
             from off-chain prices, so it cannot filter page membership. \
             Filter client-side on the `tvl` field of the returned rows.",
            serde_json::json!({ "filter": "min_tvl", "received": min }),
        );
    }

    let has_predecessor = pagination.has_predecessor();
    let direction = pagination.direction;
    // One free-text box, two things a reader can paste into it: an asset code
    // (or `A/B` pair) and a pool identifier. Try the identifier first — it is
    // the unambiguous shape, and treating it as a code found nothing, so the
    // page claimed the pool did not exist (task 0470).
    let pool_id_hex = params
        .filter_asset_code
        .as_deref()
        .and_then(pool_id_from_text);
    let asset_codes = if pool_id_hex.is_some() {
        Vec::new()
    } else {
        normalize_asset_codes(params.filter_asset_code)
    };
    // An unknown kind is a 400, never a silent pass-through: dropping the
    // filter would answer with a page that contradicts the request, which is
    // the same failure the rejected `filter[min_tvl]` produced.
    let pool_kind = match params.filter_pool_kind.as_deref() {
        None => None,
        Some(raw) => match raw.parse::<domain::PoolKind>() {
            Ok(k) => Some(k),
            Err(_) => {
                return errors::bad_request_with_details(
                    errors::INVALID_FILTER,
                    "filter[pool_kind] must be `classic` or `soroban`",
                    serde_json::json!({ "filter[pool_kind]": raw }),
                );
            }
        },
    };

    let resolved = ResolvedPoolListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        pool_kind,
        asset_codes,
        pool_id_hex,
    };

    // The CH list keys on `last_updated_ledger` (see
    // `queries::fetch_pool_list`); the sort key travels in
    // `PoolRow::cursor_ledger`.
    let fetched = queries::fetch_pool_list(&state.ch(), &resolved, direction)
        .await
        .map_err(|e| e.to_string());
    let mut rows = match fetched {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "DB error in list_pools");
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
                &PoolListCursor {
                    created_at_ledger: r.cursor_ledger,
                    pool_id_hex: r.pool_id_hex.clone(),
                },
                dir,
            )
        },
    );
    let data: Vec<PoolItem> = rows.into_iter().map(map_pool_item).collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — a classic pool's SEP-23 strkey (`L…`) or a soroban pool's contract address (`C…`), 56 chars."),
    ),
    responses(
        (status = 200, description = "Pool detail", body = PoolItem),
        (status = 400, description = "Invalid pool_id", body = ErrorEnvelope),
        (status = 404, description = "Pool not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn get_pool(State(state): State<AppState>, Path(pool_id): Path<String>) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };

    let fetched = queries::fetch_pool_by_id(&state.ch(), &pool_id_hex)
        .await
        .map_err(|e| e.to_string());
    let mut row = match fetched {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in get_pool");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // USD analytics (0199 compute-at-read): spot TVL + 24h volume/fee from
    // the in-cluster `prices.*` views. Deliberately DEGRADES to NULL fields
    // on error instead of failing the whole detail — the pool's on-chain
    // data is still valid without prices, and the FE already renders the
    // NULL ("stale") state. The error log is the operator signal (a missing
    // `prices.*` SELECT grant lands here, not in a 500).
    let ctx = queries::PoolPriceContext {
        legs: row
            .legs
            .iter()
            .map(|l| queries::price_leg(l.family, l.asset_code.as_deref(), l.issuer.as_deref()))
            .collect(),
        fee_bps: row.fee_bps,
    };
    let reserves: Vec<Option<&str>> = row.legs.iter().map(|l| l.reserve.as_deref()).collect();
    let soroban = row.pool_kind == domain::PoolKind::Soroban;
    let ch = state.ch();
    let (analytics, soroban_count) = tokio::join!(
        queries::fetch_pool_usd_analytics(&ch, &pool_id_hex, &ctx, &reserves),
        async {
            if soroban {
                queries::count_soroban_participants(&ch, &pool_id_hex).await
            } else {
                Ok(None)
            }
        },
    );
    match analytics {
        Ok(analytics) => {
            row.tvl = analytics.tvl;
            // The analytics read "no snapshot in the window" as a zero-volume
            // day. True for a classic pool, whose every trade writes a
            // snapshot; a soroban pool writes none, so its `0.00` would be a
            // claim, not a measurement — nothing records its trades yet.
            if row.pool_kind == domain::PoolKind::Classic {
                row.volume = analytics.volume;
                row.fee_revenue = analytics.fee_revenue;
            }
        }
        Err(e) => {
            tracing::error!("DB error in fetch_pool_usd_analytics({pool_id}): {e}");
        }
    }

    let mut item = map_pool_item(row);
    // A soroban pool's providers are its share token's holders — the count
    // the participants section lists. Degrades to "not indexed" on error.
    match soroban_count {
        Ok(n) if soroban => item.participant_count = n,
        Ok(_) => {}
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in count_soroban_participants");
        }
    }
    let mut resp = Json(item).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[cfg(test)]
mod normalize_asset_code_tests;

#[cfg(test)]
mod map_pool_item_tests;

#[cfg(test)]
mod participants_tests;
