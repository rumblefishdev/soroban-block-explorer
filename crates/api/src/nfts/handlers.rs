//! Axum handlers for the NFT endpoints. Pure DB — no read-time XDR.

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
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;

use super::dto::{
    ListParams, NftDetailResponse, NftItem, NftListCursor, NftTransferCursor, NftTransferItem,
};
use super::queries::{self, NftRow, ResolvedListParams};

/// Map the datasource-agnostic [`NftRow`] to the wire [`NftItem`] (drops the
/// internal `contract_surrogate` cursor tiebreak).
fn map_item(r: NftRow) -> NftItem {
    NftItem {
        contract_id: r.contract_id,
        token_id: r.token_id,
        collection_name: r.collection_name,
        name: r.name,
        media_url: r.media_url,
        minted_at_ledger: r.minted_at_ledger,
        owner_account: r.owner_account,
        last_seen_ledger: r.last_seen_ledger,
    }
}

/// Validate the composite `(:contract_id, :token_id)` NFT path parameter.
///
/// Per task 0264 Phase 8a, the external NFT route is keyed by
/// `(contract C-strkey, token_id)` — the CAP-46-6 token contract identity
/// — rather than by the internal `nfts.id i32` surrogate PK (which would
/// leak storage shape into the wire URL and break ecosystem alignment
/// with stellar.expert's contract-keyed Soroban-token addressing).
///
/// `contract` is validated as a CAP-38 `C…` StrKey via
/// `path::strkey('C', _)` (same helper used by `/v1/contracts/:id`).
///
/// `token_id` is opaque to us — the contract author chooses the shape
/// (numeric, ULID, hex, slug, …). DB column is `VARCHAR(256)`. We only
/// guard against empty and absurd lengths so a degenerate input maps to
/// 400 rather than wasting a DB round trip. 128 chars covers any
/// reasonable token_id encoding (ULID = 26, UUID = 36, u128 decimal = 39,
/// u256 hex = 64) with margin.
fn parse_nft_path(
    contract: &str,
    token_id: &str,
) -> Result<(String, String), axum::response::Response> {
    path::strkey(contract, 'C', "contract_id")?;

    if token_id.is_empty()
        || token_id.len() > 128
        || !token_id.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(errors::bad_request_with_details(
            errors::INVALID_ID,
            "token_id must be a non-empty printable-ASCII string of at most 128 characters",
            serde_json::json!({ "param": "token_id", "received": token_id }),
        ));
    }

    Ok((contract.to_string(), token_id.to_string()))
}

#[utoipa::path(
    get,
    path = "/nfts",
    tag = "nfts",
    params(
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
        ListParams,
    ),
    responses(
        (status = 200, description = "Paginated NFT list",
         body = Paginated<NftItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_nfts(
    State(state): State<AppState>,
    pagination: Pagination<NftListCursor>,
    Query(params): Query<ListParams>,
) -> Response {
    if let Err(resp) = filters::strkey_opt(params.filter_contract_id.as_deref(), 'C', "contract_id")
    {
        return resp;
    }

    if let Err(resp) = filters::reject_sql_wildcards_opt(params.filter_name.as_deref(), "name") {
        return resp;
    }

    let has_predecessor = pagination.has_predecessor();
    let direction = pagination.direction;
    let resolved = ResolvedListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        filter_collection: params.filter_collection,
        filter_contract_id: params.filter_contract_id,
        filter_name: params.filter_name,
    };

    let fetched = queries::fetch_list(&state.ch(), &resolved, direction).await;
    let mut rows = match fetched {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB error in list_nfts: {e}");
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
                &NftListCursor {
                    minted_at_ledger: r.minted_at_ledger.unwrap_or(0),
                    contract_surrogate: r.contract_surrogate,
                    token_id: r.token_id.clone(),
                },
                dir,
            )
        },
    );
    let data: Vec<NftItem> = rows.into_iter().map(map_item).collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[utoipa::path(
    get,
    path = "/nfts/{contract_id}/{token_id}",
    tag = "nfts",
    params(
        ("contract_id" = String, Path,
         description = "Contract C-StrKey hosting the NFT (CAP-46-6 token contract)."),
        ("token_id" = String, Path,
         description = "Contract-defined token id (opaque string; ≤128 ASCII chars)."),
    ),
    responses(
        (status = 200, description = "NFT detail", body = NftDetailResponse),
        (status = 400, description = "Invalid contract_id / token_id", body = ErrorEnvelope),
        (status = 404, description = "NFT not found",   body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn get_nft(
    State(state): State<AppState>,
    Path((contract_id, token_id)): Path<(String, String)>,
) -> Response {
    let (contract_id, token_id) = match parse_nft_path(&contract_id, &token_id) {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let fetched = queries::fetch_by_composite(&state.ch(), &contract_id, &token_id).await;
    let row = match fetched {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("nft not found"),
        Err(e) => {
            tracing::error!(
                contract_id = %contract_id,
                token_id = %token_id,
                "DB error fetching nft: {e}"
            );
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // NFT metadata (attributes / traits / description / animation_url)
    // is detail-only per ADR 0043 — fetched at request time via
    // `runtime_enrichment::nft_token_uri` (per-token Soroban RPC
    // `token_uri()` + HTTP / IPFS gateway → JSON). Warm requests hit
    // the in-process LRU (24 h, capacity 1024); cold requests pay the
    // RPC + IPFS round trip. The `metadata` field on the wire response
    // is independent of the fetcher module name — same convention as
    // the SEP-1 fetcher serving `description` / `home_page`.
    //
    // 3 s wall-clock cap on the cold path so a slow IPFS gateway can't
    // hold the request anywhere near the API Gateway 29 s ceiling.
    // Internally `resolve()` is already capped per leg (2 s connect /
    // 5 s request); this outer timeout bounds the combined RPC + HTTP
    // worst case. Any failure (timeout, RPC error, malformed JSON,
    // unsafe scheme) collapses to `metadata = null` — the API never
    // 5xx's because of an enrichment failure.
    let metadata = match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        state
            .runtime_enrichment
            .nft_token_uri
            .resolve(&row.contract_id, &row.token_id),
    )
    .await
    {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            tracing::warn!(
                contract_id = %row.contract_id,
                token_id = %row.token_id,
                error = %err,
                "nft_token_uri runtime fetch failed; serving metadata=null"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                contract_id = %row.contract_id,
                token_id = %row.token_id,
                "nft_token_uri runtime fetch timed out (>3s); serving metadata=null"
            );
            None
        }
    };

    let response = NftDetailResponse {
        item: row,
        metadata,
    };
    let mut resp = Json(response).into_response();
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

#[utoipa::path(
    get,
    path = "/nfts/{contract_id}/{token_id}/transfers",
    tag = "nfts",
    params(
        ("contract_id" = String, Path,
         description = "Contract C-StrKey hosting the NFT."),
        ("token_id" = String, Path,
         description = "Contract-defined token id (opaque string; ≤128 ASCII chars)."),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated NFT transfer history",
         body = Paginated<NftTransferItem>),
        (status = 400, description = "Invalid contract_id / token_id / pagination",
         body = ErrorEnvelope),
        (status = 404, description = "NFT not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn list_nft_transfers(
    State(state): State<AppState>,
    pagination: Pagination<NftTransferCursor>,
    Path((contract_id, token_id)): Path<(String, String)>,
) -> Response {
    let (contract_id, token_id) = match parse_nft_path(&contract_id, &token_id) {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let fetch_limit = pagination.fetch_limit();
    let has_predecessor = pagination.has_predecessor();
    let direction = pagination.direction;

    // Existence first (404-vs-empty disambiguation), then the page. CH keys
    // `nft_ownership` on `(contract_id, token_id)` directly, so no surrogate
    // indirection. `Ok(None)` = the NFT does not exist → 404.
    let fetched: Result<Option<Vec<NftTransferItem>>, clickhouse::error::Error> =
        match queries::nft_exists(&state.ch(), &contract_id, &token_id).await {
            Ok(true) => queries::fetch_transfers(
                &state.ch(),
                &contract_id,
                &token_id,
                pagination.cursor.as_ref(),
                fetch_limit,
                direction,
            )
            .await
            .map(Some),
            Ok(false) => Ok(None),
            Err(e) => Err(e),
        };
    let mut rows = match fetched {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("nft not found"),
        Err(e) => {
            tracing::error!(
                contract_id = %contract_id,
                token_id = %token_id,
                "DB error in nft transfers: {e}"
            );
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, last| {
            cursor::encode(
                &NftTransferCursor {
                    created_at: last.created_at,
                    ledger_sequence: last.ledger_sequence,
                    event_order: last.event_order,
                },
                dir,
            )
        },
    );

    let mut resp = Json(into_envelope(rows, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}
