//! Handlers for the liquidity-pool endpoints (participants from task 0126;
//! list / detail / transactions / chart from task 0052).

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
use crate::common::strkey::pool_id_hex_to_strkey;
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;
use crate::transactions::dto::TxListCursor;

use super::dto::{
    ChartParams, ChartResponse, ParticipantItem, PoolAssetLeg, PoolItem, PoolListCursor,
    PoolListParams, PoolTransactionItem, SharesCursor,
};
use super::queries::{self, PoolRow, PoolTxRow, ResolvedPoolListParams};

#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}/participants",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — SEP-23 strkey (`L...`, 56 chars). Internal DB form is hex (ADR 0024); strkey is the canonical wire form."),
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
// List / Detail / Transactions / Chart (task 0052)
// ---------------------------------------------------------------------------

/// Normalize `filter[asset_code]` into the needles the WHERE clause binds:
/// trim, uppercase, then split a pair query on `/`. Empty input (e.g.
/// `?filter[asset_code]=`) yields no needles — an empty one would otherwise
/// match every row (`positionCaseInsensitive(…, '') = 1`).
///
/// `USDC/XLM` becomes two needles, and the query gives each one its own leg in
/// either order — so the typed order does not matter, and one asset cannot
/// satisfy both halves (`USDC/USDC` means both legs, not "USDC anywhere, twice
/// over"). The split is
/// `splitn(2)` on purpose: this is a *pair* filter, and an unbounded split would
/// let a caller turn one long free-text field into thousands of needles, each
/// costing a pass over the table. A third code therefore lands inside needle two
/// (`XLM/BTC`), which matches nothing — correct, since a pool has two legs, and
/// honest, since nothing was silently discarded.
///
/// The DB side matches each needle as a case-insensitive **substring** of either
/// leg (0440), so the uppercasing here is belt-and-braces rather than load-bearing;
/// the trim and the empty-needle drop are what the query depends on.
///
/// Stellar protocol asset codes are case-sensitive (1–12 ASCII chars,
/// any case), but the canonical convention is uppercase (USDC, XLM). The
/// trim+uppercase normalization matches caller intent for the list's free-text
/// field; consumers who need exact case-sensitive issuer-disambiguated matching
/// should use the per-leg `filter[asset_a_code]` / `filter[asset_a_issuer]` mode
/// instead.
fn normalize_asset_codes(raw: Option<String>) -> Vec<String> {
    raw.map(|s| s.trim().to_uppercase())
        .into_iter()
        .flat_map(|s| {
            s.splitn(2, '/')
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn map_pool_item(row: PoolRow) -> PoolItem {
    PoolItem {
        pool_id: pool_id_hex_to_strkey(&row.pool_id_hex),
        asset_a: PoolAssetLeg {
            asset_type_name: row.asset_a_type_name,
            asset_type: row.asset_a_type,
            asset_code: row.asset_a_code,
            issuer: row.asset_a_issuer,
            contract_id: row.asset_a_contract_id,
            icon_url: row.asset_a_icon_url,
        },
        asset_b: PoolAssetLeg {
            asset_type_name: row.asset_b_type_name,
            asset_type: row.asset_b_type,
            asset_code: row.asset_b_code,
            issuer: row.asset_b_issuer,
            contract_id: row.asset_b_contract_id,
            icon_url: row.asset_b_icon_url,
        },
        fee_bps: row.fee_bps,
        fee_percent: row.fee_percent,
        created_at_ledger: row.created_at_ledger,
        participant_count: row.participant_count,
        latest_snapshot_ledger: row.latest_snapshot_ledger,
        reserve_a: row.reserve_a,
        reserve_b: row.reserve_b,
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
    if let Err(resp) = filters::strkey_opt(
        params.filter_asset_a_issuer.as_deref(),
        'G',
        "asset_a_issuer",
    ) {
        return resp;
    }
    if let Err(resp) = filters::strkey_opt(
        params.filter_asset_b_issuer.as_deref(),
        'G',
        "asset_b_issuer",
    ) {
        return resp;
    }

    // Asset-leg filter pairing: classic identity is `(code, issuer)`. Native
    // legs have no code AND no issuer. Mixed (one set, one absent) is
    // ambiguous — canonical SQL 18 §46-49 says "API validates inputs
    // upstream"; this is that validator. Without it, `?filter[asset_a_code]=USDC`
    // alone would match every USDC-coded pool regardless of issuer (the wrong
    // USDC issuer included).
    let a_code_set = params.filter_asset_a_code.is_some();
    let a_issuer_set = params.filter_asset_a_issuer.is_some();
    if a_code_set != a_issuer_set {
        return errors::bad_request_with_details(
            errors::INVALID_FILTER,
            "filter[asset_a_code] and filter[asset_a_issuer] must be supplied together \
             (classic identity) or both omitted",
            serde_json::json!({
                "filter[asset_a_code]": params.filter_asset_a_code,
                "filter[asset_a_issuer]": params.filter_asset_a_issuer,
            }),
        );
    }
    let b_code_set = params.filter_asset_b_code.is_some();
    let b_issuer_set = params.filter_asset_b_issuer.is_some();
    if b_code_set != b_issuer_set {
        return errors::bad_request_with_details(
            errors::INVALID_FILTER,
            "filter[asset_b_code] and filter[asset_b_issuer] must be supplied together \
             (classic identity) or both omitted",
            serde_json::json!({
                "filter[asset_b_code]": params.filter_asset_b_code,
                "filter[asset_b_issuer]": params.filter_asset_b_issuer,
            }),
        );
    }

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
    let resolved = ResolvedPoolListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        asset_a_code: params.filter_asset_a_code,
        asset_a_issuer: params.filter_asset_a_issuer,
        asset_b_code: params.filter_asset_b_code,
        asset_b_issuer: params.filter_asset_b_issuer,
        asset_codes: normalize_asset_codes(params.filter_asset_code),
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
         description = "Pool ID — SEP-23 strkey (`L...`, 56 chars). Internal DB form is hex (ADR 0024); strkey is the canonical wire form."),
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
        leg_a: queries::price_leg(
            row.asset_a_type,
            row.asset_a_code.as_deref(),
            row.asset_a_issuer.as_deref(),
        ),
        leg_b: queries::price_leg(
            row.asset_b_type,
            row.asset_b_code.as_deref(),
            row.asset_b_issuer.as_deref(),
        ),
        fee_bps: row.fee_bps,
    };
    match queries::fetch_pool_usd_analytics(
        &state.ch(),
        &pool_id_hex,
        &ctx,
        row.reserve_a.as_deref(),
        row.reserve_b.as_deref(),
    )
    .await
    {
        Ok(analytics) => {
            row.tvl = analytics.tvl;
            row.volume = analytics.volume;
            row.fee_revenue = analytics.fee_revenue;
        }
        Err(e) => {
            tracing::error!("DB error in fetch_pool_usd_analytics({pool_id}): {e}");
        }
    }

    let mut resp = Json(map_pool_item(row)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

/// `true` when the decoded cursor is a current (CH) cursor. A stale cursor
/// minted under the retired PG backend is rejected as `invalid_cursor`.
fn pool_tx_cursor_matches_source(cursor: &TxListCursor) -> bool {
    matches!(cursor, TxListCursor::Ch { .. })
}

/// Build the next/prev cursor from a boundary row. CH keys on
/// `(ledger_sequence, transaction_id)` — `transaction_id` == `transactions.id`.
fn pool_tx_cursor_for(r: &PoolTxRow) -> TxListCursor {
    TxListCursor::Ch {
        ledger_sequence: r.ledger_sequence,
        tiebreak: r.id,
    }
}

#[utoipa::path(
    get,
    path = "/liquidity-pools/{pool_id}/transactions",
    tag = "liquidity-pools",
    params(
        ("pool_id" = String, Path,
         description = "Pool ID — SEP-23 strkey (`L...`, 56 chars)."),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated pool transactions",
         body = Paginated<PoolTransactionItem>),
        (status = 400, description = "Invalid pool_id, limit, or cursor", body = ErrorEnvelope),
        (status = 404, description = "Pool not found",  body = ErrorEnvelope),
        (status = 500, description = "Database error",  body = ErrorEnvelope),
    )
)]
pub async fn list_pool_transactions(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
    pagination: Pagination<TxListCursor>,
) -> Response {
    let pool_id_hex = match path::pool_id_strkey(&pool_id, "pool_id") {
        Ok(hex) => hex,
        Err(resp) => return resp,
    };

    // Reject a stale cursor minted under the retired PG backend: its keyset
    // values are meaningless under CH, so fail with `invalid_cursor` rather
    // than silently mis-paginating (ADR 0008). Mirrors `transactions::list`.
    if let Some(cursor) = pagination.cursor.as_ref()
        && !pool_tx_cursor_matches_source(cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    // The pool's two leg surrogates, which double as this path's existence
    // check (task 0279): the rows' `asset_id` maps onto them, so the response
    // can carry `amount_a` / `amount_b` aligned with the legs the page already
    // renders — one seek instead of a separate `pool_exists`.
    //
    // Stays SERIAL: the page read now CONSUMES `asset_ids`, so there is nothing
    // to overlap. This supersedes task 0446's pairing of the old `pool_exists`
    // gate with the page — a gate that also carries data is strictly better
    // than two queries run concurrently.
    let legs = queries::fetch_pool_asset_ids(&state.ch(), &pool_id_hex)
        .await
        .map_err(|e| e.to_string());
    let asset_ids = match legs {
        Ok(Some(ids)) => ids,
        Ok(None) => return errors::not_found("liquidity pool not found"),
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_asset_ids");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let fetched = queries::fetch_pool_transactions(
        &state.ch(),
        &pool_id_hex,
        asset_ids,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        pagination.direction,
    )
    .await
    .map_err(|e| e.to_string());
    let mut rows = match fetched {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(pool_id = %pool_id, error = %e, "DB error in fetch_pool_transactions");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Cursor payload differs by datasource (PG keys on `(created_at, id)`;
    // CH on `(ledger_sequence, transaction_id)`) but stays opaque on the wire.
    let page = finalize_page(
        &mut rows,
        pagination.limit,
        pagination.direction,
        pagination.has_predecessor(),
        |dir, r| cursor::encode(&pool_tx_cursor_for(r), dir),
    );
    let data: Vec<PoolTransactionItem> = rows
        .into_iter()
        .map(|r| PoolTransactionItem {
            hash: r.hash,
            ledger_sequence: r.ledger_sequence,
            source_account: r.source_account,
            fee_charged: r.fee_charged,
            successful: r.successful,
            operation_count: r.operation_count,
            has_soroban: r.has_soroban,
            operation_types: r.operation_types,
            created_at: r.created_at,
            amounts: r.amounts,
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
         description = "Pool ID — SEP-23 strkey (`L...`, 56 chars)."),
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
mod normalize_asset_code_tests {
    use super::normalize_asset_codes;

    #[test]
    fn none_passes_through() {
        assert!(normalize_asset_codes(None).is_empty());
    }

    #[test]
    fn empty_string_becomes_none() {
        assert!(normalize_asset_codes(Some(String::new())).is_empty());
        assert!(normalize_asset_codes(Some("   ".into())).is_empty());
    }

    #[test]
    fn lowercase_is_uppercased() {
        assert_eq!(normalize_asset_codes(Some("usdc".into())), ["USDC"]);
    }

    #[test]
    fn mixed_case_is_uppercased() {
        assert_eq!(normalize_asset_codes(Some("UsDc".into())), ["USDC"]);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(normalize_asset_codes(Some("  xlm  ".into())), ["XLM"]);
    }

    #[test]
    fn pair_splits_into_two_needles() {
        assert_eq!(
            normalize_asset_codes(Some("usdc/xlm".into())),
            ["USDC", "XLM"]
        );
    }

    #[test]
    fn pair_tolerates_spaces_around_the_slash() {
        assert_eq!(
            normalize_asset_codes(Some(" usdc / xlm ".into())),
            ["USDC", "XLM"]
        );
    }

    #[test]
    fn half_written_pair_keeps_the_written_half() {
        // Mid-typing state: the field debounces and fires on `USDC/`.
        assert_eq!(normalize_asset_codes(Some("USDC/".into())), ["USDC"]);
        assert_eq!(normalize_asset_codes(Some("/XLM".into())), ["XLM"]);
        assert!(normalize_asset_codes(Some("/".into())).is_empty());
    }

    #[test]
    fn third_code_stays_inside_the_second_needle() {
        // `splitn(2)` bounds the needle count. The remainder is not discarded —
        // it becomes a needle no asset code can contain, so the query returns
        // nothing rather than silently answering a narrower question.
        assert_eq!(
            normalize_asset_codes(Some("USDC/XLM/BTC".into())),
            ["USDC", "XLM/BTC"]
        );
        assert!(normalize_asset_codes(Some("/".repeat(5_000))).len() <= 2);
    }

    #[test]
    fn unicode_lower_uppercases_too() {
        // Stellar codes are ASCII-only in practice, but the normalizer
        // should not panic on UTF-8 — `String::to_uppercase` handles it.
        assert_eq!(normalize_asset_codes(Some("usdc🪙".into())), ["USDC🪙"]);
    }
}

#[cfg(test)]
mod map_pool_item_tests {
    use super::*;
    use crate::liquidity_pools::queries::PoolRow;

    fn base_row() -> PoolRow {
        PoolRow {
            pool_id_hex: "0".repeat(64),
            asset_a_type: 0,
            asset_a_type_name: Some("native".into()),
            asset_a_code: None,
            asset_a_issuer: None,
            asset_a_contract_id: None,
            asset_a_icon_url: None,
            asset_b_type: 1,
            asset_b_type_name: Some("credit_alphanum4".into()),
            asset_b_code: Some("USDC".into()),
            asset_b_issuer: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
            asset_b_contract_id: None,
            asset_b_icon_url: None,
            fee_bps: 30,
            fee_percent: "0.30".into(),
            created_at_ledger: 100,
            cursor_ledger: 100,
            participant_count: 0,
            latest_snapshot_ledger: None,
            reserve_a: None,
            reserve_b: None,
            total_shares: None,
            tvl: None,
            volume: None,
            fee_revenue: None,
            latest_snapshot_at: None,
        }
    }

    #[test]
    fn native_leg_has_no_contract_id() {
        let item = map_pool_item(base_row());
        assert_eq!(item.asset_a.asset_type, 0, "asset_a is native");
        assert_eq!(item.asset_a.contract_id, None);
        assert_eq!(item.asset_b.asset_type, 1, "asset_b is classic credit");
    }

    #[test]
    fn icon_url_propagates_per_leg() {
        // gap #5: each leg's icon_url threads from the row to the DTO leg,
        // independently. Native leg (no icon) stays None.
        let mut row = base_row();
        row.asset_b_icon_url = Some("https://cdn.example.test/icons/usdc.svg".into());
        let item = map_pool_item(row);
        assert_eq!(item.asset_a.icon_url, None, "native leg has no icon");
        assert_eq!(
            item.asset_b.icon_url.as_deref(),
            Some("https://cdn.example.test/icons/usdc.svg")
        );
    }

    #[test]
    fn classic_credit_leg_surfaces_issuer_and_no_sac_mirror() {
        let item = map_pool_item(base_row());
        assert_eq!(item.asset_b.asset_code.as_deref(), Some("USDC"));
        assert!(item.asset_b.issuer.is_some());
        assert_eq!(
            item.asset_b.contract_id, None,
            "no SAC mirror in `assets` → contract_id stays None"
        );
    }

    #[test]
    fn sac_mirror_contract_id_propagates_to_response() {
        let mut row = base_row();
        row.asset_b_contract_id =
            Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB".into());
        let item = map_pool_item(row);
        assert_eq!(
            item.asset_b.contract_id.as_deref(),
            Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB")
        );
    }
}
