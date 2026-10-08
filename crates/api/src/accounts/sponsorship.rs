//! `GET /v1/accounts/:account_id/sponsorship` — who pays each sponsored
//! reserve of the account (CAP-33, issue #454), read live from Soroban RPC.
//!
//! Its own request, not part of the account detail: it waits on RPC, and the
//! page should not.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};

use crate::common::{cache_control, errors, path};
use crate::openapi::schemas::ErrorEnvelope;
use crate::state::AppState;

use super::dto::{AccountSponsoredEntry, AccountSponsorshipResponse};
use super::queries;

#[utoipa::path(
    get,
    path = "/accounts/{account_id}/sponsorship",
    tag = "accounts",
    params(
        ("account_id" = String, Path, description = "Stellar account StrKey (G…, 56 chars)"),
    ),
    responses(
        (status = 200, description = "The account's sponsored entries and who pays them",
         body = AccountSponsorshipResponse),
        (status = 400, description = "Invalid account_id", body = ErrorEnvelope),
        (status = 404, description = "Account not found, or no entry on the ledger", body = ErrorEnvelope),
        (status = 500, description = "Database or RPC failure", body = ErrorEnvelope),
    ),
)]
pub async fn get_account_sponsorship(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Response {
    if let Err(resp) = path::strkey(&account_id, 'G', "account_id") {
        return resp;
    }
    let header = match queries::fetch_account(&state.ch(), &account_id).await {
        Ok(Some(h)) => h,
        Ok(None) => return errors::not_found(format!("account '{account_id}' not found")),
        Err(e) => {
            tracing::error!(account_id = %account_id, error = %e, "DB error fetching account");
            return errors::internal_error(errors::DB_ERROR, "database error");
        }
    };
    // RPC cannot list an account's trustlines; our open classic holdings name
    // them, and RPC then says who pays each.
    let trustlines: Vec<(String, String)> =
        match queries::fetch_balances(&state.ch(), header.id).await {
            Ok(rows) => rows
                .into_iter()
                // Classic credit: the only holdings that are trustlines.
                .filter(|r| r.asset_type == domain::AssetFamily::ClassicCredit as i16)
                .filter_map(|r| Some((r.asset_code?, r.asset_issuer?)))
                .collect(),
            Err(e) => {
                tracing::error!(account_id = %account_id, error = %e, "DB error fetching balances");
                return errors::internal_error(errors::DB_ERROR, "database error");
            }
        };
    let sponsors = match state
        .runtime_enrichment
        .account_sponsors
        .fetch(&account_id, &trustlines)
        .await
    {
        Ok(Some(s)) => s,
        Ok(None) => return errors::not_found("the account has no entry on the ledger"),
        Err(e) => {
            tracing::error!(account_id = %account_id, error = %e, "RPC error fetching sponsors");
            return errors::internal_error(
                errors::SPONSORSHIP_FETCH_FAILED,
                "could not read sponsors from RPC",
            );
        }
    };
    let body = AccountSponsorshipResponse {
        num_sponsored: sponsors.num_sponsored,
        entries: sponsors
            .entries
            .into_iter()
            .map(|e| AccountSponsoredEntry {
                kind: e.kind.to_string(),
                asset: e.asset,
                signer: e.signer,
                reserves: e.reserves,
                sponsor: e.sponsor,
            })
            .collect(),
    };
    let mut resp = Json(body).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}
