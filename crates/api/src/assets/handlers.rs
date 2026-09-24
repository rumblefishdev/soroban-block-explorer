//! Axum handlers for the assets endpoints. Pure DB — no read-time XDR.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use domain::AssetFamily;

use crate::common::cache_control;
use crate::common::cursor;
use crate::common::cursor::Direction;
use crate::common::errors;
use crate::common::extractors::Pagination;
use crate::common::filters;
use crate::common::pagination::{finalize_page, into_envelope};
use crate::common::strkey::is_strkey_shape;
use crate::openapi::schemas::{ErrorEnvelope, PageInfo, Paginated};
use crate::runtime_enrichment::sep1::{Sep1Currency, Sep1TomlParsed};
use crate::state::AppState;
use crate::transactions::dto::TxListCursor;

use super::dto::{
    AssetDetailResponse, AssetItem, AssetKeyCursor, AssetTransactionItem, ListParams,
};
use super::queries::{self, AssetRow, AssetTxRow, ListedAsset, ResolvedListParams};

/// The list cursor: the rank the SEEK ordered this row by (already folded,
/// `-1` for no aggregate row), never the `holder_count` hydration read again —
/// the two can come from different `balance_aggregates` rebuilds.
fn listed_asset_cursor(dir: Direction, r: &ListedAsset) -> String {
    cursor::encode(
        &AssetKeyCursor {
            holder_rank: r.holder_rank,
            id: r.row.id,
        },
        dir,
    )
}

async fn fetch_list_for_source(
    state: &AppState,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<ListedAsset>, clickhouse::error::Error> {
    queries::fetch_list(&state.ch(), params, direction).await
}

/// `:id` detail-row dispatch — resolves the `AssetRow` from CH.
async fn fetch_asset_row_for_source(
    state: &AppState,
    parsed: AssetIdRef<'_>,
) -> Result<Option<AssetRow>, clickhouse::error::Error> {
    match parsed {
        AssetIdRef::Native => queries::fetch_native(&state.ch()).await,
        AssetIdRef::Contract(c) => queries::fetch_by_contract_id(&state.ch(), c).await,
        AssetIdRef::CodeIssuer(code, issuer) => {
            queries::fetch_by_code_issuer(&state.ch(), code, issuer).await
        }
    }
}

/// Canonical wire id — the single token usable as `/assets/{id}`: the contract
/// StrKey for a `soroban` asset (the contract IS the asset), the reserved
/// `native` token for XLM, else the `CODE-ISSUER` composite (classic credit).
/// A SAC-wrapped classic / native asset keys off CODE-ISSUER / `native` (ADR
/// 0051 — the SAC handle is a facet, not the identity), so the empty fallback
/// is unreachable in practice.
fn canonical_id(row: &AssetRow) -> String {
    if let Some(contract_id) = &row.contract_id {
        return contract_id.clone();
    }
    if let (Some(code), Some(issuer)) = (&row.asset_code, &row.issuer) {
        return format!("{code}-{issuer}");
    }
    // Native XLM singleton (asset_type 0): no composite identity → reserved token.
    if row.asset_type == 0 {
        return "native".to_string();
    }
    String::new()
}

/// Map a fetched row to the wire item, re-deriving the SAC `C…` StrKey on read
/// (ADR 0051 — never stored) from `code:issuer` when the asset carries an
/// observed SAC facet (`sac_contract_surrogate != 0`).
fn map_item(row: AssetRow, network_id: &[u8; 32]) -> AssetItem {
    let (sac_contract_id, sac_deployed) = if row.sac_contract_surrogate != 0 {
        let code = row.asset_code.as_deref().unwrap_or("");
        let issuer = row.issuer.as_deref().unwrap_or("");
        (
            xdr_parser::derive_sac_strkey(code, issuer, network_id),
            Some(row.sac_deployed),
        )
    } else {
        (None, None)
    };
    AssetItem {
        id: canonical_id(&row),
        asset_type_name: row.asset_type_name,
        asset_type: row.asset_type,
        asset_code: row.asset_code,
        issuer: row.issuer,
        issuer_home_domain: row.issuer_home_domain,
        contract_id: row.contract_id,
        sac_contract_id,
        sac_deployed,
        name: row.name,
        symbol: row.symbol,
        decimals: row.decimals,
        total_supply: row.total_supply,
        holder_count: row.holder_count,
        icon_url: row.icon_url,
    }
}

/// Forms of `:id` (the numeric surrogate was dropped — PR #175 / the composite
/// move). The first that parses cleanly drives the SQL. `native` is a reserved
/// token: the classic native XLM singleton (`asset_type = 0`) carries no
/// composite identity (no contract_id, no code/issuer per `ck_assets_identity`),
/// so it has no StrKey / CODE-ISSUER to address it by.
enum AssetIdRef<'a> {
    Native,
    Contract(&'a str),
    CodeIssuer(&'a str, &'a str),
}

fn parse_asset_id(raw: &str) -> Option<AssetIdRef<'_>> {
    if raw.eq_ignore_ascii_case("native") {
        return Some(AssetIdRef::Native);
    }
    if is_strkey_shape(raw, 'C') {
        return Some(AssetIdRef::Contract(raw));
    }
    // Codes never contain `-`; split on the LAST one and validate the
    // issuer half as a G-StrKey to disambiguate from C-StrKeys with stray dashes.
    if let Some(idx) = raw.rfind('-')
        && idx > 0
        && idx < raw.len() - 1
    {
        let code = &raw[..idx];
        let issuer = &raw[idx + 1..];
        if is_strkey_shape(issuer, 'G') {
            return Some(AssetIdRef::CodeIssuer(code, issuer));
        }
    }
    None
}

#[utoipa::path(
    get,
    path = "/assets",
    tag = "assets",
    params(
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
        ListParams,
    ),
    responses(
        (status = 200, description = "Paginated asset list",
         body = Paginated<AssetItem>),
        (status = 400, description = "Invalid query parameter", body = ErrorEnvelope),
        (status = 500, description = "Internal server error",   body = ErrorEnvelope),
    ),
)]
pub async fn list_assets(
    State(state): State<AppState>,
    pagination: Pagination<AssetKeyCursor>,
    Query(params): Query<ListParams>,
) -> Response {
    let asset_type: Option<i16> = match filters::parse_enum_opt::<AssetFamily>(
        params.filter_type.as_deref(),
        "type",
        Some("asset type"),
    ) {
        Ok(maybe) => maybe.map(|t| t as i16),
        Err(resp) => return resp,
    };

    if let Err(resp) = filters::reject_sql_wildcards_opt(params.filter_code.as_deref(), "code") {
        return resp;
    }

    // SAC property filter (ADR 0051): `filter[sac]=true` → the SAC view.
    let sac_only = params
        .filter_sac
        .as_deref()
        .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");

    let direction = pagination.direction;
    let has_predecessor = pagination.has_predecessor();
    let resolved = ResolvedListParams {
        limit: pagination.fetch_limit(),
        cursor: pagination.cursor,
        asset_type,
        asset_code: params.filter_code,
        sac_only,
    };

    let mut rows: Vec<ListedAsset> = match fetch_list_for_source(&state, &resolved, direction).await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "DB error in list_assets");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        direction,
        has_predecessor,
        listed_asset_cursor,
    );
    let data: Vec<AssetItem> = rows
        .into_iter()
        .map(|r| map_item(r.row, &state.network_id))
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[utoipa::path(
    get,
    path = "/assets/{id}",
    tag = "assets",
    params(
        ("id" = String, Path,
         description = "Contract StrKey (C…, 56 chars), `CODE-ISSUER` composite (e.g. USDC-GA…), or the reserved `native` token for XLM."),
    ),
    responses(
        (status = 200, description = "Asset detail", body = AssetDetailResponse),
        (status = 400, description = "Invalid id format", body = ErrorEnvelope),
        (status = 404, description = "Asset not found",   body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn get_asset(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let parsed = match parse_asset_id(&id) {
        Some(p) => p,
        None => {
            return errors::bad_request_with_details(
                errors::INVALID_ID,
                "id must be a contract StrKey (C…, 56 chars), \
                 a `CODE-ISSUER` composite (e.g. USDC-GA…XYZ), \
                 or the reserved `native` token for XLM",
                serde_json::json!({ "received": id }),
            );
        }
    };

    let row = match fetch_asset_row_for_source(&state, parsed).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("asset not found"),
        Err(e) => {
            tracing::error!(asset_id = %id, error = %e, "DB error fetching asset");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let deployed_at_ledger = row.deployed_at_ledger;
    let issuer = row.issuer.clone();
    let asset_code = row.asset_code.clone();
    let home_domain = row.issuer_home_domain.clone();

    // SEP-1 runtime enrichment (task 0188). `description` ← matched
    // `CURRENCIES[].desc`; `home_page` ← `DOCUMENTATION.ORG_URL`. Native
    // XLM / no-issuer Soroban tokens / accounts without `home_domain`
    // skip the fetch and surface both as `null`. A real fetch failure
    // also yields `null` (warn-logged) — the API never propagates a 5xx
    // because of an enrichment failure.
    let (description, home_page) = match home_domain.as_deref() {
        Some(domain) if !domain.is_empty() => {
            match state.runtime_enrichment.sep1.fetch(domain).await {
                Ok(parsed) => {
                    extract_sep1_fields(&parsed, asset_code.as_deref(), issuer.as_deref())
                }
                Err(e) => {
                    tracing::warn!(home_domain = %domain, error = %e, "SEP-1 fetch failed for issuer home_domain");
                    (None, None)
                }
            }
        }
        _ => (None, None),
    };

    let response = AssetDetailResponse {
        item: map_item(row, &state.network_id),
        deployed_at_ledger,
        description,
        home_page,
    };
    let mut resp = Json(response).into_response();
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

/// Pull the two SEP-1 fields the API exposes: `desc` from the matching
/// `CURRENCIES[]` row (by `code` + `issuer`) and `ORG_URL` from
/// `DOCUMENTATION` (used as `home_page` since SEP-1 has no per-currency
/// homepage field). A missing currency match still yields the org URL —
/// useful when the issuer publishes their site but doesn't list every
/// individual token.
fn extract_sep1_fields(
    parsed: &Sep1TomlParsed,
    asset_code: Option<&str>,
    issuer: Option<&str>,
) -> (Option<String>, Option<String>) {
    let description = match (asset_code, issuer) {
        (Some(code), Some(iss)) => {
            find_currency(&parsed.currencies, code, iss).and_then(|c| c.desc.clone())
        }
        _ => None,
    };
    let home_page = parsed
        .documentation
        .as_ref()
        .and_then(|d| d.org_url.clone());
    (description, home_page)
}

/// Find the `CURRENCIES[]` entry whose `code` and `issuer` match the
/// queried asset. Returns the first match (SEP-1 does not require codes
/// to be unique, but in practice they are per issuer).
fn find_currency<'a>(
    currencies: &'a [Sep1Currency],
    asset_code: &str,
    issuer: &str,
) -> Option<&'a Sep1Currency> {
    currencies
        .iter()
        .find(|c| c.code.as_deref() == Some(asset_code) && c.issuer.as_deref() == Some(issuer))
}

/// `:id/transactions` dispatch — the `operation_asset_appearances` fan-out on
/// `id` (task 0359): operations naming the asset and token events moving it.
async fn fetch_asset_tx_for_source(
    state: &AppState,
    row: &AssetRow,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<AssetTxRow>, clickhouse::error::Error> {
    queries::fetch_transactions(&state.ch(), row.id, limit, cursor, direction).await
}

/// Build the opaque asset-transactions cursor for a boundary row. The list keys
/// on the transaction's position `(ledger_sequence, application_order)` (task
/// 0575).
fn asset_tx_cursor_for(r: &AssetTxRow) -> TxListCursor {
    TxListCursor::ChPosition {
        ledger_sequence: r.ledger_sequence,
        application_order: r.application_order,
    }
}

/// True when the cursor anchors this list's keyset, the position. A surrogate
/// cursor is refused (ADR 0008 fail-clean).
fn cursor_matches_source(cursor: &TxListCursor) -> bool {
    matches!(cursor, TxListCursor::ChPosition { .. })
}

#[utoipa::path(
    get,
    path = "/assets/{id}/transactions",
    tag = "assets",
    params(
        ("id" = String, Path,
         description = "Contract StrKey (C…, 56 chars), `CODE-ISSUER` composite (e.g. USDC-GA…), or the reserved `native` token for XLM."),
        ("limit" = Option<u32>, Query,
         description = "Items per page (1–100, default 20).",
         minimum = 1, maximum = 100),
        ("cursor" = Option<String>, Query,
         description = "Opaque pagination cursor from a previous response."),
    ),
    responses(
        (status = 200, description = "Paginated transactions involving the asset",
         body = Paginated<AssetTransactionItem>),
        (status = 400, description = "Invalid id format / pagination", body = ErrorEnvelope),
        (status = 404, description = "Asset not found", body = ErrorEnvelope),
        (status = 500, description = "Internal server error", body = ErrorEnvelope),
    ),
)]
pub async fn list_asset_transactions(
    State(state): State<AppState>,
    pagination: Pagination<TxListCursor>,
    Path(id): Path<String>,
) -> Response {
    let parsed = match parse_asset_id(&id) {
        Some(p) => p,
        None => {
            return errors::bad_request_with_details(
                errors::INVALID_ID,
                "id must be a contract StrKey (C…, 56 chars), \
                 a `CODE-ISSUER` composite (e.g. USDC-GA…XYZ), \
                 or the reserved `native` token for XLM",
                serde_json::json!({ "received": id }),
            );
        }
    };

    // Reject a stale cursor minted under the retired PG backend — its keyset is
    // meaningless under CH (ADR 0008 fail-clean). A legacy/untagged cursor
    // already fails decode upstream; this guards the decodes-but-wrong-intent
    // case.
    if let Some(cursor) = &pagination.cursor
        && !cursor_matches_source(cursor)
    {
        return errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired");
    }

    let row = match fetch_asset_row_for_source(&state, parsed).await {
        Ok(Some(r)) => r,
        Ok(None) => return errors::not_found("asset not found"),
        Err(e) => {
            tracing::error!(asset_id = %id, error = %e, "DB error fetching asset");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    // ADR 0051 / task 0339: discriminant 2 (retired `sac`) no longer exists in
    // prod (Phase-2 relabel complete) and is rejected by `try_from` like any
    // other unknown discriminant.
    if AssetFamily::try_from(row.asset_type).is_err() {
        tracing::error!(
            asset_type = row.asset_type,
            contract_id = ?row.contract_id,
            asset_code = ?row.asset_code,
            "unknown asset_type discriminant for asset"
        );
        return errors::internal_error(
            errors::DB_ERROR,
            "asset row carries an unknown asset_type discriminant",
        );
    }

    // Guard the unresolved-asset sentinel: a resolved asset always carries a real
    // `ids::asset_id` surrogate, so `id == 0` means "no such asset key" — return an
    // empty page rather than run `WHERE asset_id = 0`. Native is NOT caught here:
    // it has a first-class surrogate (`ids::asset_id(0,"",0,0)`, non-zero), so the
    // fan-out seek runs and native activity is served (task 0359 / devils-advocate
    // C6, PR #9 — this replaces the old identity gate that emptied native because
    // it has no classic code/issuer/contract).
    if row.id == 0 {
        let empty = into_envelope::<AssetTransactionItem>(
            Vec::new(),
            PageInfo {
                next_cursor: None,
                prev_cursor: None,
                limit: pagination.limit,
            },
        );
        let mut resp = Json(empty).into_response();
        cache_control::attach(&mut resp, cache_control::SHORT);
        return resp;
    }

    let mut rows = match fetch_asset_tx_for_source(
        &state,
        &row,
        pagination.fetch_limit(),
        pagination.cursor.as_ref(),
        pagination.direction,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(asset_id = %id, error = %e, "DB error in list_asset_transactions");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };

    let page = finalize_page(
        &mut rows,
        pagination.limit,
        pagination.direction,
        pagination.has_predecessor(),
        |dir, r| cursor::encode(&asset_tx_cursor_for(r), dir),
    );
    let data: Vec<AssetTransactionItem> = rows
        .into_iter()
        .map(|r| AssetTransactionItem {
            hash: r.hash,
            ledger_sequence: r.ledger_sequence,
            source_account: r.source_account,
            successful: r.successful,
            fee_charged: r.fee_charged,
            created_at: r.created_at,
            operation_count: r.operation_count,
            has_soroban: r.has_soroban,
            operation_types: r.operation_types,
        })
        .collect();

    let mut resp = Json(into_envelope(data, page)).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[cfg(test)]
mod tests;
