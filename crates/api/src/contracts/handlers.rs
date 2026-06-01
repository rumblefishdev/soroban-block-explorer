//! Axum handlers for the contracts endpoints.
//! Mirrors the transactions / assets pattern: `common::*` for pagination,
//! errors, cursor codec, and StrKey validation (task 0043).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use stellar_xdr::curr::{LedgerCloseMeta, TransactionMeta};
use xdr_parser::EventSource;

use crate::common::cache_control;
use crate::common::cursor::{self, Direction, TsIdCursor};
use crate::common::datasource::{DataSource, Module};
use crate::common::extractors::Pagination;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::{errors, path};
use crate::openapi::schemas::{ErrorEnvelope, Paginated};
use crate::runtime_enrichment::stellar_archive::extractors::collect_tx_metas;
use crate::state::AppState;
use crate::transactions::dto::TxListCursor;

use super::dto::{
    ContractDetailResponse, ContractStats, EventItem, InterfaceResponse, InvocationItem,
};
use super::queries::{
    ContractRow, EventAppearanceRow, InterfaceRow, InvocationAppearanceRow, fetch_contract,
    fetch_contract_stats, fetch_event_appearances, fetch_invocation_appearances,
    fetch_wasm_interface,
};
use super::queries_ch;

/// Unified per-call fetch error so the handlers dispatch between PG and CH
/// without leaking driver types. Only `Display` is observed.
#[derive(Debug, thiserror::Error)]
enum CtrFetchError {
    #[error("pg: {0}")]
    Pg(sqlx::Error),
    #[error("ch: {0}")]
    Ch(clickhouse::error::Error),
}

/// Default time window for `fetch_contract_stats` (canonical 11 Statement B).
const STATS_WINDOW: &str = "7 days";

async fn fetch_unique_ledgers(
    state: &AppState,
    sequences: &[i64],
) -> HashMap<u32, LedgerCloseMeta> {
    let unique_seqs: Vec<u32> = {
        let mut seen = HashSet::new();
        sequences
            .iter()
            .filter_map(|s| match u32::try_from(*s) {
                Ok(seq) => seen.insert(seq).then_some(seq),
                Err(_) => {
                    tracing::warn!("skipping out-of-u32-range ledger_sequence {s}");
                    None
                }
            })
            .collect()
    };

    let results = state
        .runtime_enrichment
        .stellar_archive
        .fetch_ledgers(&unique_seqs)
        .await;
    unique_seqs
        .into_iter()
        .zip(results)
        .filter_map(|(seq, res)| match res {
            Ok(meta) => Some((seq, meta)),
            Err(e) => {
                tracing::warn!("failed to fetch ledger {seq} from public archive: {e}");
                None
            }
        })
        .collect()
}

/// Parser outputs reused by the events handler. E13 dropped XDR per
/// canonical 13's "out of scope" note, so this is events-only now.
struct ParsedLedger<'a> {
    ledger_sequence: u32,
    closed_at: i64,
    tx_metas: Vec<&'a TransactionMeta>,
    tx_index: HashMap<String, usize>,
}

impl<'a> ParsedLedger<'a> {
    fn new(meta: &'a LedgerCloseMeta, network_id: &[u8; 32]) -> Option<Self> {
        let ledger = xdr_parser::extract_ledger(meta);
        let extracted_txs =
            xdr_parser::extract_transactions(meta, ledger.sequence, ledger.closed_at, network_id);
        let tx_metas = collect_tx_metas(meta);
        let tx_index: HashMap<String, usize> = extracted_txs
            .iter()
            .enumerate()
            .map(|(i, t)| (t.hash.clone(), i))
            .collect();
        Some(Self {
            ledger_sequence: ledger.sequence,
            closed_at: ledger.closed_at,
            tx_metas,
            tx_index,
        })
    }

    fn tx_index_by_hash(&self, tx_hash: &str) -> Option<usize> {
        self.tx_index.get(tx_hash).copied()
    }
}

fn build_parsed_ledgers<'a>(
    ledger_map: &'a HashMap<u32, LedgerCloseMeta>,
    network_id: &[u8; 32],
) -> HashMap<u32, ParsedLedger<'a>> {
    ledger_map
        .iter()
        .filter_map(|(seq, meta)| {
            let parsed = ParsedLedger::new(meta, network_id)?;
            Some((*seq, parsed))
        })
        .collect()
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

    let source = DataSource::for_module(Module::Contracts);

    let contract = match fetch_contract_for_source(&state, source, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!(source = ?source, "DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let (recent_invocations, recent_unique_callers, stats_window) =
        match fetch_stats_for_source(&state, source, contract.id, STATS_WINDOW).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(source = ?source, "DB error fetching stats for {contract_id}: {e}");
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
        // `metadata` field removed per ADR 0042 / task 0156.
        stats: ContractStats {
            recent_invocations,
            recent_unique_callers,
            stats_window,
        },
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

    let source = DataSource::for_module(Module::Contracts);

    // 200 + interface_metadata=null for SAC / pre-upload / stub-only;
    // 404 only when the contract row itself is missing.
    let row = match fetch_interface_for_source(&state, source, &contract_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!(source = ?source, "DB error fetching interface for {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let mut resp = Json(InterfaceResponse {
        contract_id: row.contract_id,
        wasm_hash: row.wasm_hash,
        interface_metadata: row.interface_metadata,
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

    let source = DataSource::for_module(Module::Contracts);

    // Reject a cursor minted for the other datasource (ADR 0008 fail-clean).
    if let Some(cursor) = &pagination.cursor
        && !cursor_matches_source(source, cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    let contract = match fetch_contract_for_source(&state, source, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!(source = ?source, "DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let mut rows: Vec<InvocationAppearanceRow> = match fetch_invocations_for_source(
        &state,
        source,
        contract.id,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(source = ?source, "DB error in list_invocations: {e}");
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
        |dir, r| cursor::encode(&invocation_cursor_for(source, r), dir),
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
         description = "Items per page (1–100, default 20). Page granularity is per\n`(contract, transaction, ledger)` appearance — a single appearance\ncan expand to multiple per-node items in the response, so the\nreturned `data.len()` may exceed `limit`.",
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
    pagination: Pagination<TsIdCursor>,
    Path(contract_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&contract_id, 'C', "contract_id") {
        return resp;
    }

    let contract = match fetch_contract(&state.db, &contract_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return errors::not_found("contract not found"),
        Err(e) => {
            tracing::error!("DB error fetching contract {contract_id}: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let mut rows: Vec<EventAppearanceRow> = match fetch_event_appearances(
        &state.db,
        contract.id,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("DB error in list_events: {e}");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // Cursor matrix (truncate excess + reverse-on-Prev) handled by the
    // canonical helper. `rows` mutates in place: post-call it carries
    // exactly the displayed page in DESC presentation order regardless
    // of fetch direction.
    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        |dir, r| cursor::encode(&TsIdCursor::new(r.created_at, r.transaction_id), dir),
    );

    // Archive XDR fetch (runtime, ADR 0029). Hard-fail policy: if any
    // requested ledger is missing from the response, surface 500 — the
    // explorer treats archive outages as fail-fast rather than silently
    // dropping events or stalling cursor. Per project policy: assume S3
    // doesn't fail; when it does, the client retries.
    let sequences: Vec<i64> = rows.iter().map(|r| r.ledger_sequence).collect();
    let ledger_map = fetch_unique_ledgers(&state, &sequences).await;
    let parsed = build_parsed_ledgers(&ledger_map, &state.network_id);

    let items = match expand_events(&rows, &parsed, &contract_id) {
        Ok(v) => v,
        Err(()) => {
            tracing::error!("archive XDR expansion failed for contract {contract_id}");
            return errors::internal_error(errors::ARCHIVE_ERROR, "archive XDR fetch failed");
        }
    };

    let mut resp = Json(into_envelope(items, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

/// Expand appearance rows into wire [`EventItem`]s using the archive
/// XDR fetched into `parsed`. Returns `Err(())` on any gap (missing
/// ledger, missing tx, missing tx_meta) so the handler can return a
/// proper 500. Per project policy archive failures are hard, but they
/// surface as graceful HTTP errors — not panics.
fn expand_events(
    rows: &[EventAppearanceRow],
    parsed: &HashMap<u32, ParsedLedger<'_>>,
    contract_id: &str,
) -> Result<Vec<EventItem>, ()> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let seq = u32::try_from(row.ledger_sequence).map_err(|_| ())?;
        let ledger = parsed.get(&seq).ok_or(())?;
        let idx = ledger.tx_index_by_hash(&row.transaction_hash).ok_or(())?;
        let tm = ledger.tx_metas.get(idx).copied().ok_or(())?;
        let events = xdr_parser::extract_events(
            tm,
            &row.transaction_hash,
            ledger.ledger_sequence,
            ledger.closed_at,
        );
        for event in events {
            if event.contract_id.as_deref() != Some(contract_id) {
                continue;
            }
            // Drop the entire diagnostic_events container — its
            // Contract-typed entries are byte-identical mirrors of the
            // per-op consensus events (task 0182). Filtering by inner
            // `event_type` would surface those mirrors as duplicates.
            if event.source == EventSource::Diagnostic {
                continue;
            }
            let topics = match event.topics {
                serde_json::Value::Array(a) => a,
                other => vec![other],
            };
            items.push(EventItem {
                transaction_hash: row.transaction_hash.clone(),
                ledger_sequence: row.ledger_sequence,
                transaction_id: row.transaction_id,
                successful: row.successful,
                amount: row.amount,
                created_at: row.created_at,
                event_type: event.event_type.to_string(),
                topics,
                data: event.data,
            });
        }
    }
    Ok(items)
}

// ---------------------------------------------------------------------------
// Per-source dispatch helpers (task 0243)
//
// `events` (canonical 14) has no CH path yet (inline-XDR ScVal decode +
// event_index keyset — see `queries_ch`), so `list_events` stays PG-only and
// `API_DATASOURCE_CONTRACTS=ch` must remain off until it lands. The detail /
// interface / invocations endpoints dispatch through these helpers.
// ---------------------------------------------------------------------------

async fn fetch_contract_for_source(
    state: &AppState,
    source: DataSource,
    contract_id: &str,
) -> Result<Option<ContractRow>, CtrFetchError> {
    match source {
        DataSource::Pg => fetch_contract(&state.db, contract_id)
            .await
            .map_err(CtrFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_contract(state.ch(), contract_id)
            .await
            .map_err(CtrFetchError::Ch),
    }
}

async fn fetch_stats_for_source(
    state: &AppState,
    source: DataSource,
    contract_surrogate_id: i64,
    window: &str,
) -> Result<(i64, i64, String), CtrFetchError> {
    match source {
        DataSource::Pg => fetch_contract_stats(&state.db, contract_surrogate_id, window)
            .await
            .map_err(CtrFetchError::Pg),
        DataSource::Ch => {
            queries_ch::fetch_contract_stats(state.ch(), contract_surrogate_id, window)
                .await
                .map_err(CtrFetchError::Ch)
        }
    }
}

async fn fetch_interface_for_source(
    state: &AppState,
    source: DataSource,
    contract_id: &str,
) -> Result<Option<InterfaceRow>, CtrFetchError> {
    match source {
        DataSource::Pg => fetch_wasm_interface(&state.db, contract_id)
            .await
            .map_err(CtrFetchError::Pg),
        DataSource::Ch => queries_ch::fetch_wasm_interface(state.ch(), contract_id)
            .await
            .map_err(CtrFetchError::Ch),
    }
}

async fn fetch_invocations_for_source(
    state: &AppState,
    source: DataSource,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<InvocationAppearanceRow>, CtrFetchError> {
    match source {
        DataSource::Pg => {
            // PG keys on `(created_at, transaction_id)`; the cross-source guard
            // guarantees any present cursor is the `Pg` variant.
            let ts_cursor = cursor.and_then(|c| match c {
                TxListCursor::Pg { ts, id } => Some(TsIdCursor::new(*ts, *id)),
                TxListCursor::Ch { .. } => None,
            });
            fetch_invocation_appearances(
                &state.db,
                contract_surrogate_id,
                limit,
                ts_cursor.as_ref(),
                direction,
            )
            .await
            .map_err(CtrFetchError::Pg)
        }
        DataSource::Ch => queries_ch::fetch_invocation_appearances(
            state.ch(),
            contract_surrogate_id,
            limit,
            cursor,
            direction,
        )
        .await
        .map_err(CtrFetchError::Ch),
    }
}

/// Build the opaque invocations cursor for a boundary row. PG keys on
/// `(created_at, id)`; CH keys on `(ledger_sequence, id)` (the
/// `soroban_invocations_appearances` keyset). Tagged with the active datasource
/// so a later request rejects a cursor minted for the other backend.
fn invocation_cursor_for(source: DataSource, r: &InvocationAppearanceRow) -> TxListCursor {
    match source {
        DataSource::Pg => TxListCursor::Pg {
            ts: r.created_at,
            id: r.transaction_id,
        },
        DataSource::Ch => TxListCursor::Ch {
            ledger_sequence: r.ledger_sequence,
            tiebreak: r.transaction_id,
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
