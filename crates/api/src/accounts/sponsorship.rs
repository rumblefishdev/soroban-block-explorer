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

/// Wall-clock cap on the RPC reads of one request.
const FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

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
    // One RPC endpoint may hold a call for 10 s before the pool tries the
    // next, so 20 s leaves room for one failover. Uncapped, four endpoints at
    // 10 s each, times up to five calls, would pass the API Gateway's 29 s
    // ceiling.
    let fetched = tokio::time::timeout(
        FETCH_DEADLINE,
        state
            .runtime_enrichment
            .account_sponsors
            .fetch(&account_id, &trustlines),
    )
    .await;
    let sponsors = match fetched {
        Ok(Ok(Some(s))) => s,
        Ok(Ok(None)) => return errors::not_found("the account has no entry on the ledger"),
        Ok(Err(e)) => {
            tracing::error!(account_id = %account_id, error = %e, "RPC error fetching sponsors");
            return errors::internal_error(
                errors::SPONSORSHIP_FETCH_FAILED,
                "could not read sponsors from RPC",
            );
        }
        Err(_elapsed) => {
            tracing::error!(account_id = %account_id, "RPC timed out fetching sponsors");
            return errors::internal_error(
                errors::SPONSORSHIP_FETCH_FAILED,
                "could not read sponsors from RPC in time",
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
    // MEDIUM like the other runtime fetches (NFT metadata, SEP-1): each miss
    // costs RPC calls, and sponsorships change far less often than a minute.
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}
