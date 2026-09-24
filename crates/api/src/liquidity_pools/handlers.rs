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
use crate::common::filters;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::path;
use crate::common::pool_asset_codes::normalize_asset_codes;
use crate::common::strkey::{pool_id_from_text, pool_id_hex_to_strkey};
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;

use super::dto::{
    ChartParams, ChartResponse, ParticipantItem, PoolActivityCursor, PoolActivityItem,
    PoolActivityParams, PoolAssetLeg, PoolEvent, PoolItem, PoolListCursor, PoolListParams,
    SharesCursor,
};
use super::queries::{self, PoolLegRow, PoolRow, ResolvedPoolListParams};

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
        (status = 400, description = "Invalid pool_id, limit, or cursor", body = ErrorEnvelope),
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
    let ch = state.ch();
    let (exists, fetched) = tokio::join!(
        queries::pool_exists(&ch, &pool_id_hex),
        queries::fetch_participants(
            &ch,
            &pool_id_hex,
            pagination.cursor.as_ref(),
            fetch_limit,
            direction,
        ),
    );
    match exists.map_err(|e| e.to_string()) {
        Ok(true) => {}
        Ok(false) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in pool_exists");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    }

    let mut rows = match fetched.map_err(|e| e.to_string()) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_participants");
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
                    shares: last.shares.clone(),
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
    match queries::fetch_pool_usd_analytics(
        &state.ch(),
        &pool_id_hex,
        &ctx,
        &row.legs
            .iter()
            .map(|l| l.reserve.as_deref())
            .collect::<Vec<_>>(),
    )
    .await
    {
        Ok(analytics) => {
            row.tvl = analytics.tvl;
            // Volume is read off the classic snapshot. Nothing records a
            // soroban pool's (its state changes carry reserves only), and an
            // empty window reads as a zero-volume day — "$0.00 traded" would
            // be an invented measurement, so the fields stay unknown.
            if row.pool_kind == domain::PoolKind::Classic {
                row.volume = analytics.volume;
                row.fee_revenue = analytics.fee_revenue;
            }
        }
        Err(e) => {
            tracing::error!("DB error in fetch_pool_usd_analytics({pool_id}): {e}");
        }
    }

    let mut resp = Json(map_pool_item(row)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

/// The `allowed` list a `filter[event]` rejection returns. Derived from the
/// enum's own spellings rather than retyped, so it cannot advertise a value
/// `PoolEvent::from_param` would then refuse.
const ALLOWED_EVENTS: [&str; 3] = [
    PoolEvent::Trade.as_param(),
    PoolEvent::Deposit.as_param(),
    PoolEvent::Withdrawal.as_param(),
];

/// `GET /v1/liquidity-pools/{pool_id}/activity` — the pool's operations
/// (task 0491, issue #371).
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
        (status = 200, description = "Paginated pool activity, one row per operation",
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

    // The pool's leg surrogates, which double as this path's existence
    // check: the driver pivots `lp_operation_amounts.asset_id` onto them, so
    // the read cannot run without them and a missing pool is one seek away
    // (task 0279's pairing, kept).
    let legs = queries::fetch_pool_asset_ids(&state.ch(), &pool_id_hex)
        .await
        .map_err(|e| e.to_string());
    let leg_ids = match legs {
        Ok(Some(ids)) => ids,
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

    let fetched = queries::fetch_pool_activity(
        &state.ch(),
        &pool_id_hex,
        &leg_ids,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        pagination.direction,
        event,
    )
    .await
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
                    transaction_id: r.transaction_id,
                    application_order: r.application_order,
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
            application_order: r.application_order,
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

const ALLOWED_INTERVALS: &[&str] = &["1h", "1d", "1w"];

/// Hard cap on the number of buckets a single chart request can produce.
///
/// Without a cap a malicious / buggy caller could request a 10-year window
/// at `interval=1h` (≈ 87 600 buckets), which forces the planner into a
/// large GROUP BY + ARRAY_AGG aggregation. 1 000 buckets covers every
/// realistic UI need (≈ 41 days at 1h, ≈ 2.7 years at 1d, ≈ 19 years at
/// 1w) and stays cheap on the snapshots index.
const MAX_CHART_BUCKETS: i64 = 1_000;

/// Approximate bucket width in seconds for each allowlisted interval.
/// Used only for the bucket-count guard before SQL — `date_trunc`
/// computes the actual buckets.
fn interval_seconds(interval: &str) -> i64 {
    match interval {
        "1h" => 3_600,
        "1d" => 86_400,
        "1w" => 604_800,
        // unreachable — handler validates against ALLOWED_INTERVALS first.
        _ => 1,
    }
}

#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}/chart",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — a classic pool's SEP-23 strkey (`L…`) or a soroban pool's contract address (`C…`), 56 chars."),
        ChartParams,
    ),
    responses(
        (status = 200, description = "Time-bucketed pool chart series", body = ChartResponse),
        (status = 400, description = "Invalid pool_id / interval / from / to", body = ErrorEnvelope),
        (status = 404, description = "Pool not found", body = ErrorEnvelope),
        (status = 500, description = "Database error", body = ErrorEnvelope),
    ),
)]
pub async fn get_pool_chart(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    Query(params): Query<ChartParams>,
) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };

    // All three params are optional. Defaults are tuned per interval so a
    // bare `?` request produces a useful chart without bucket-cap
    // violations:
    //   1h → last 7 days     (168 buckets)
    //   1d → last 90 days    ( 90 buckets, ≈ 3 months)
    //   1w → last 104 weeks  (104 buckets, ≈ 2 years)
    let interval = match params.interval.as_deref() {
        Some(s) if ALLOWED_INTERVALS.contains(&s) => s.to_string(),
        Some(s) => {
            return errors::bad_request_with_details(
                errors::INVALID_FILTER,
                "interval must be one of: 1h, 1d, 1w",
                serde_json::json!({
                    "param": "interval",
                    "received": s,
                    "allowed": ALLOWED_INTERVALS,
                }),
            );
        }
        None => "1d".to_string(),
    };

    let to = match params.to.as_deref() {
        Some(v) => match filters::parse_iso8601(v, "to") {
            Ok(d) => d,
            Err(resp) => return resp,
        },
        None => chrono::Utc::now(),
    };
    let from = match params.from.as_deref() {
        Some(v) => match filters::parse_iso8601(v, "from") {
            Ok(d) => d,
            Err(resp) => return resp,
        },
        None => {
            // Default window matches the interval — see comment above.
            let back = match interval.as_str() {
                "1h" => chrono::Duration::days(7),
                "1d" => chrono::Duration::days(90),
                "1w" => chrono::Duration::weeks(104),
                _ => unreachable!("interval already validated against allowlist"),
            };
            to - back
        }
    };
    if from >= to {
        return errors::bad_request_with_details(
            errors::INVALID_FILTER,
            "from must be strictly before to",
            serde_json::json!({ "from": from.to_rfc3339(), "to": to.to_rfc3339() }),
        );
    }

    // Bucket-count guard: reject ranges that would force the aggregation
    // beyond `MAX_CHART_BUCKETS`. `date_trunc` aligns buckets to wall-clock
    // boundaries — a span that crosses a boundary mid-interval produces
    // one extra bucket. Ceil division covers the "span just under N
    // intervals" case; `+ 1` covers the wall-clock alignment case.
    let interval_secs = interval_seconds(&interval);
    let span_seconds = (to - from).num_seconds();
    // Manual ceil division (`i64::div_ceil` is still unstable as of stable
    // Rust 2024). `+ 1` covers the wall-clock alignment edge.
    let approx_buckets = (span_seconds + interval_secs - 1) / interval_secs + 1;
    if approx_buckets > MAX_CHART_BUCKETS {
        return errors::bad_request_with_details(
            errors::INVALID_FILTER,
            format!(
                "(to - from) at interval={interval} would produce ~{approx_buckets} buckets; \
                 maximum is {MAX_CHART_BUCKETS}"
            ),
            serde_json::json!({
                "interval": interval,
                "approx_buckets": approx_buckets,
                "max_buckets": MAX_CHART_BUCKETS,
                "from": from.to_rfc3339(),
                "to": to.to_rfc3339(),
            }),
        );
    }

    // Doubles as the 404 existence gate (one row on `liquidity_pools`) and
    // supplies the leg identities + fee_bps the USD computation joins on.
    //
    // Stays SERIAL, unlike the gates in `list_participants` /
    // `list_pool_transactions` (task 0446), and for two independent reasons.
    // The chart read now CONSUMES `ctx`, so it is genuinely dependent — nothing
    // to overlap. It also could not have been paired even before that: its
    // `JOIN (SELECT … FROM ledgers WHERE closed_at …)` build side is
    // materialised even when the left side is empty, and `MAX_CHART_BUCKETS`
    // admits a ~19-year window, so speculatively running it cost a measured
    // 43.7M rows / 4.66 s for a pool that does not exist, against 16.5k rows /
    // 3.6 ms for the gate. Pool ids are user-supplied strkeys. If a future
    // change breaks the data dependency, that measurement still stands.
    let ctx = match queries::fetch_pool_price_context(&state.ch(), &pool_id_hex).await {
        Ok(Some(ctx)) => ctx,
        Ok(None) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_price_context");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let fetched = queries::fetch_pool_chart(&state.ch(), &pool_id_hex, &ctx, &interval, from, to)
        .await
        .map_err(|e| e.to_string());
    let data_points = match fetched {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_chart");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let mut resp = Json(ChartResponse {
        pool_id,
        interval,
        from,
        to,
        data_points,
    })
    .into_response();
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

#[cfg(test)]
mod normalize_asset_code_tests;

#[cfg(test)]
mod map_pool_item_tests;
