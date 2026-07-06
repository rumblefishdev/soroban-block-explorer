//! Axum handlers for the contracts endpoints.
//! Mirrors the transactions / assets pattern: `common::*` for pagination,
//! errors, cursor codec, and StrKey validation (task 0043).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use domain::ContractType;

use crate::common::cache_control;
use crate::common::cursor::{self, Direction};
use crate::common::extractors::Pagination;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::{errors, filters, path};
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::state::AppState;
use crate::transactions::dto::TxListCursor;

use super::dto::{
    ContractDetailResponse, ContractIdCursor, ContractInterfaceMetadata, ContractListItem,
    ContractListRow, ContractRow, ContractStats, ContractsListParams, EventCursor, EventItem,
    InterfaceResponse, InterfaceRow, InvocationAppearanceRow, InvocationItem,
    ResolvedContractsListParams, STATS_WINDOW,
};
use super::queries_ch;

// ---------------------------------------------------------------------------
// GET /v1/contracts (list)
// ---------------------------------------------------------------------------

/// List contracts, newest-ingested first (`id DESC`, the PK order — no
/// user sort). `filter[type]` narrows by class, `filter[q]` searches
/// id/name. Cursor-paginated like every other list endpoint.
#[utoipa::path(
    get,
    path = "/contracts",
    tag = "contracts",
    params(
        ("limit"  = Option<u32>,    Query, description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor from a previous response."),
        ContractsListParams,
    ),
    responses(
        (status = 200, description = "Paginated contract list",
         body = Paginated<ContractListItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_contracts(
    State(state): State<AppState>,
    pagination: Pagination<ContractIdCursor>,
    Query(params): Query<ContractsListParams>,
) -> Response {
    let contract_type: Option<i16> = match filters::parse_enum_opt::<ContractType>(
        params.filter_type.as_deref(),
        "type",
        Some("contract type"),
    ) {
        Ok(maybe) => maybe.map(|t| t as i16),
        Err(resp) => return resp,
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let resolved = ResolvedContractsListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        contract_type,
        q: params.filter_q,
    };

    let mut rows: Vec<ContractListRow> =
        match fetch_contract_list_for_source(&state, &resolved, direction).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("DB error in list_contracts: {e}");
                return errors::internal_error(errors::DB_ERROR, "database error");
            }
        };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, r| cursor::encode(&ContractIdCursor { id: r.id }, dir),
    );
    let data: Vec<ContractListItem> = rows.into_iter().map(map_contract_list_item).collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

fn map_contract_list_item(r: ContractListRow) -> ContractListItem {
    ContractListItem {
        contract_id: r.contract_id,
        contract_type: r.contract_type,
        contract_type_name: r.contract_type_name,
        is_sac: r.is_sac,
        deployer: r.deployer,
        deployed_at_ledger: r.deployed_at_ledger,
        recent_invocations: r.recent_invocations,
    }
}

async fn fetch_contract_list_for_source(
    state: &AppState,
    params: &ResolvedContractsListParams,
    direction: Direction,
) -> Result<Vec<ContractListRow>, clickhouse::error::Error> {
    queries_ch::fetch_contract_list(&state.ch(), params, direction).await
}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars)"),
    ),
    responses(
        (status = 200, description = "Contract detail", body = ContractDetailResponse),
        (status = 400, description = "Invalid contract_id",  body = ErrorEnvelope),
        (status = 404, description = "Contract not found",  body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn get_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&contract_id, 'C', "contract_id") {
        return resp;
    }

    if let Some(cached) = state.contract_cache.get(&contract_id) {
        let mut resp = Json(cached).into_response();
        cache_control::attach(&mut resp, cache_control::MEDIUM);
        return resp;
    }

    let contract = match fetch_contract_for_source(&state, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!("DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let stats = match fetch_stats_for_source(&state, contract.id, STATS_WINDOW).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("DB error fetching stats for {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let response = Arc::new(ContractDetailResponse {
        contract_id: contract.contract_id,
        wasm_hash: contract.wasm_hash,
        wasm_uploaded_at_ledger: contract.wasm_uploaded_at_ledger,
        deployer: contract.deployer,
        deployed_at_ledger: contract.deployed_at_ledger,
        contract_type_name: contract.contract_type_name,
        contract_type: contract.contract_type,
        is_sac: contract.is_sac,
        // Task 0327 — mutability, 3-state (CH-only; None/Unknown on the retired PG
        // path). Resolved in `fetch_contract` from the joined WASM interface
        // metadata; no extra round-trip.
        upgradeable: contract.upgradeable,
        stats,
    });

    state
        .contract_cache
        .insert(contract_id, Arc::clone(&response));
    let mut resp = Json(response).into_response();
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/interface",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars)"),
    ),
    responses(
        (status = 200, description = "Public function signatures", body = InterfaceResponse),
        (status = 400, description = "Invalid contract_id",  body = ErrorEnvelope),
        (status = 404, description = "Contract not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn get_interface(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&contract_id, 'C', "contract_id") {
        return resp;
    }

    // 200 + interface_metadata=null for SAC / pre-upload / stub-only;
    // 404 only when the contract row itself is missing.
    let row = match fetch_interface_for_source(&state, &contract_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!("DB error fetching interface for {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Decode JSONB into the typed schema. `None` (no `functions` key) is a
    // legit empty (SAC / stub). Present-but-unparseable is shape drift or a
    // legacy-shape row — fail LOUD (500) so it is noticed and fixed,
    // not silently degraded to `interface_metadata: null`.
    let interface_metadata = match row
        .interface_metadata
        .map(serde_json::from_value::<ContractInterfaceMetadata>)
        .transpose()
    {
        Ok(m) => m,
        Err(err) => {
            tracing::error!(
                contract_id = %row.contract_id,
                "interface_metadata present but failed to decode — shape drift between indexer output and the API DTO, or a legacy-shape row needing re-index: {err}"
            );
            return errors::internal_error(
                errors::INTERFACE_METADATA_CORRUPT,
                "interface metadata could not be decoded",
            );
        }
    };
    let mut resp = Json(InterfaceResponse {
        contract_id: row.contract_id,
        wasm_hash: row.wasm_hash,
        interface_metadata,
    })
    .into_response();
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/invocations",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars)"),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20). One DB appearance row maps to\none `InvocationItem` (no expansion), so `data.len() <= limit`.",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated invocation appearance index",
         body = Paginated<InvocationItem>),
        (status = 400, description = "Invalid contract_id / limit / cursor", body = ErrorEnvelope),
        (status = 404, description = "Contract not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn list_invocations(
    State(state): State<AppState>,
    pagination: Pagination<TxListCursor>,
    Path(contract_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&contract_id, 'C', "contract_id") {
        return resp;
    }

    // Reject a stale cursor minted under the retired PG backend (ADR 0008
    // fail-clean).
    if let Some(cursor) = &pagination.cursor
        && !cursor_matches_source(cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    let contract = match fetch_contract_for_source(&state, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!("DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let mut rows: Vec<InvocationAppearanceRow> = match fetch_invocations_for_source(
        &state,
        contract.id,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB error in list_invocations: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Standard bidirectional pagination — invocations have no expansion
    // ambiguity (one DB row → one wire item), so the canonical helper
    // does the cursor matrix + reverse-on-Prev work.
    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, r| cursor::encode(&invocation_cursor_for(r), dir),
    );

    let data: Vec<InvocationItem> = rows
        .into_iter()
        .map(|row| InvocationItem {
            transaction_hash: row.transaction_hash,
            ledger_sequence: row.ledger_sequence,
            caller_account: row.caller_account,
            amount: row.amount,
            created_at: row.created_at,
            successful: row.successful,
        })
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/events",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars)"),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20). On the PG datasource the page is per\n`(contract, transaction, ledger)` appearance — one appearance can expand to\nmultiple events, so `data.len()` may exceed `limit`. On the CH datasource the\npage is per event (one row → one item), so `data.len() <= limit`.",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated event history",
         body = Paginated<EventItem>),
        (status = 400, description = "Invalid contract_id / limit / cursor", body = ErrorEnvelope),
        (status = 404, description = "Contract not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn list_events(
    State(state): State<AppState>,
    pagination: Pagination<EventCursor>,
    Path(contract_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&contract_id, 'C', "contract_id") {
        return resp;
    }

    // Reject a stale cursor minted under the retired PG backend (ADR 0008
    // fail-clean) — its keyset is not interchangeable with the CH keyset
    // `(ledger_sequence, id, event_index)`.
    if let Some(cursor) = &pagination.cursor
        && !event_cursor_matches_source(cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    let contract = match fetch_contract_for_source(&state, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!("DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();

    // Full-content `soroban_events` (per-event rows, inline JSON payload) —
    // one row → one `EventItem`, no Archive overlay. Keyset is 3-component
    // `(ledger_sequence, id, event_index)`.
    let mut rows = match queries_ch::fetch_events(
        &state.ch(),
        contract.id,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("CH error in list_events: {e}");
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
                &EventCursor::Ch {
                    ledger_sequence: r.item.ledger_sequence,
                    transaction_id: r.item.transaction_id,
                    event_index: r.event_index,
                },
                dir,
            )
        },
    );
    let items: Vec<EventItem> = rows.into_iter().map(|r| r.item).collect();
    let mut resp = Json(into_envelope(items, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

/// True when the decoded events cursor is a current (CH) cursor. A stale cursor
/// minted under the retired PG backend is rejected (ADR 0008 fail-clean).
fn event_cursor_matches_source(cursor: &EventCursor) -> bool {
    matches!(cursor, EventCursor::Ch { .. })
}

// ---------------------------------------------------------------------------
// Fetch helpers. Events read CH `soroban_events` directly (full-content,
// inline JSON payload); see `queries_ch::fetch_events`.
// ---------------------------------------------------------------------------

async fn fetch_contract_for_source(
    state: &AppState,
    contract_id: &str,
) -> Result<Option<ContractRow>, clickhouse::error::Error> {
    queries_ch::fetch_contract(&state.ch(), contract_id).await
}

async fn fetch_stats_for_source(
    state: &AppState,
    contract_surrogate_id: i64,
    window: &str,
) -> Result<ContractStats, clickhouse::error::Error> {
    queries_ch::fetch_contract_stats(&state.ch(), contract_surrogate_id, window).await
}

async fn fetch_interface_for_source(
    state: &AppState,
    contract_id: &str,
) -> Result<Option<InterfaceRow>, clickhouse::error::Error> {
    queries_ch::fetch_wasm_interface(&state.ch(), contract_id).await
}

async fn fetch_invocations_for_source(
    state: &AppState,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    queries_ch::fetch_invocation_appearances(
        &state.ch(),
        contract_surrogate_id,
        limit,
        cursor,
        direction,
    )
    .await
}

/// Build the opaque invocations cursor for a boundary row. CH keys on
/// `(ledger_sequence, id)` (the `soroban_invocations_appearances` keyset).
fn invocation_cursor_for(r: &InvocationAppearanceRow) -> TxListCursor {
    TxListCursor::Ch {
        ledger_sequence: r.ledger_sequence,
        tiebreak: r.transaction_id,
    }
}

/// True when the decoded cursor is a current (CH) cursor. A stale cursor minted
/// under the retired PG backend is rejected (ADR 0008 fail-clean).
fn cursor_matches_source(cursor: &TxListCursor) -> bool {
    matches!(cursor, TxListCursor::Ch { .. })
}
