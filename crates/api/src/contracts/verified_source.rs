//! `GET /v1/contracts/{id}/verification` and `…/verified-source` (task 0639).
//!
//! Same shape as `/decompiled`: the contract's current Wasm hash comes from
//! our database (CAP-85 references resolved), the answer from an outside
//! service on request, nothing is stored, and the response carries a cache
//! header. The status is small and fetched with the page; the files are
//! fetched only when the Verified Code tab opens.

// Helpers return the error `Response` itself, as in `common::path`.
#![allow(clippy::result_large_err)]

/// An AWS Lambda response is limited to 6 MB; leave room for headers.
const MAX_RESPONSE_BYTES: usize = 5_500_000;

/// The limit counts the API Gateway proxy response, where the JSON body
/// travels as one JSON string: every `"` and `\` in it gains an escape byte
/// (serde_json output holds no raw control characters to escape).
fn proxy_body_len(json: &[u8]) -> usize {
    json.len() + json.iter().filter(|&&b| b == b'"' || b == b'\\').count()
}

fn fits_lambda_response(json: &[u8]) -> bool {
    proxy_body_len(json) <= MAX_RESPONSE_BYTES
}

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};

use crate::common::{cache_control, errors, path};
use crate::openapi::schemas::ErrorEnvelope;
use crate::runtime_enrichment::source_verification::archive::{self, ArchiveError, SourceFile};
use crate::runtime_enrichment::source_verification::{
    self, Aggregate, SourceVerificationFetcher, VerificationStatus, VerifierResult, build_option,
    report_url,
};
use crate::state::AppState;

use super::dto::{
    BuildRecipe, SourceVerificationResponse, SourceVerifier, VerifiedSourceArchive,
    VerifiedSourceFile, VerifiedSourceResponse,
};
use super::queries;

/// The contract, its current Wasm hash and the aggregator's answer for it —
/// the shared first half of both endpoints. `Err` is the response to send.
struct Lookup {
    contract_id: String,
    wasm_hash: String,
    aggregate: Aggregate,
    /// Kept for `/verified-source`, which downloads the archive with it.
    fetcher: SourceVerificationFetcher,
}

async fn lookup(state: &AppState, contract_id: &str) -> Result<Lookup, Response> {
    let contract_id = &path::contract_id(contract_id)?;
    // Unset on networks without a verifier service. `/verification` answers
    // `unavailable` before getting here; for the files it is a 404.
    let Some(fetcher) = state.runtime_enrichment.source_verification.clone() else {
        return Err(errors::not_found(
            "source verification is not available on this network",
        ));
    };
    let row = match queries::fetch_wasm_interface(&state.ch(), contract_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return Err(errors::not_found("contract not found")),
        Err(e) => {
            tracing::error!("DB error resolving wasm_hash for {contract_id}: {e}");
            return Err(errors::internal_error(errors::DB_ERROR, "database error"));
        }
    };
    let Some(wasm_hash) = row.wasm_hash else {
        return Err(errors::not_found(
            "contract has no wasm (SAC or pre-upload)",
        ));
    };
    let aggregate = match source_verification::fetch_aggregate(&fetcher, &wasm_hash).await {
        Ok(agg) => agg,
        Err(e) => {
            tracing::error!(
                "source verification lookup failed for {contract_id} ({wasm_hash}): {e}"
            );
            return Err(errors::internal_error(
                errors::SOURCE_VERIFICATION_FAILED,
                "the source verification service could not be read",
            ));
        }
    };
    Ok(Lookup {
        contract_id: row.contract_id,
        wasm_hash,
        aggregate,
        fetcher,
    })
}

/// The page's status and the record it shows; see `source_verification::reduce`.
fn reduce(l: &Lookup) -> Result<(VerificationStatus, Option<VerifierResult>), Response> {
    match source_verification::reduce(&l.aggregate) {
        Ok(reduced) => Ok(reduced),
        Err(e) => {
            tracing::error!(
                "source verification answer unusable for {} ({}): {e}",
                l.contract_id,
                l.wasm_hash
            );
            Err(errors::internal_error(
                errors::SOURCE_VERIFICATION_FAILED,
                "the source verification service gave no usable answer",
            ))
        }
    }
}

/// The verifier whose published archive the Lambda may fetch, with that
/// archive, or `None` when no verifier publishes one. Every verifier that
/// verified the code keeps its own copy of the same bytes, so the first
/// usable copy is taken — and the recipe shown beside it comes from the same
/// verifier, never another's. When copies exist but none is usable, the
/// last refusal is the error.
fn archive_of(
    records: &[VerifierResult],
) -> Result<Option<(&VerifierResult, VerifiedSourceArchive)>, ArchiveError> {
    let mut last_err = None;
    for r in records {
        if r.status != "verified" {
            continue;
        }
        let (url, format) = match archive::pick(&r.verified_source_code_uris, &r.verifier.url) {
            Ok(Some(picked)) => picked,
            Ok(None) => continue,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let Some(sha256) = &r.source_sha256 else {
            last_err = Some(ArchiveError::Corrupt(
                "published archive without source_sha256".into(),
            ));
            continue;
        };
        let published = VerifiedSourceArchive {
            url,
            // As published; compared without regard to case.
            sha256: sha256.clone(),
            format: format.to_owned(),
        };
        return Ok(Some((r, published)));
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

/// Download the archive, check it against its published sha256, unpack it.
async fn fetch_files(
    fetcher: &SourceVerificationFetcher,
    published: &VerifiedSourceArchive,
) -> Result<Vec<SourceFile>, ArchiveError> {
    let bytes = archive::download(&fetcher.client, &published.url).await?;
    archive::check_sha256(&bytes, &published.sha256)?;
    archive::unpack(&bytes, &published.format)
}

/// A download that failed in transit is the service's failure; anything
/// else means the archive itself did not pass.
fn archive_error(contract_id: &str, e: ArchiveError) -> Response {
    tracing::error!("verified source archive rejected for {contract_id}: {e}");
    match e {
        ArchiveError::Download(_) => errors::internal_error(
            errors::SOURCE_VERIFICATION_FAILED,
            "the verified source archive could not be downloaded",
        ),
        _ => errors::internal_error(
            errors::SOURCE_ARCHIVE_INVALID,
            "the verified source archive failed its checks",
        ),
    }
}

// ---------------------------------------------------------------------------
// GET /v1/contracts/{contract_id}/verification
// ---------------------------------------------------------------------------

/// What independent verifiers say about the contract's current Wasm.
///
/// Read from the verifier aggregator on request. `unavailable` on a network
/// without a verifier service; 404 for an unknown contract and for one
/// without Wasm.
#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/verification",
    tag = "contracts",
    params(("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars), or the same contract with a sub-account id (W…, 69 chars, CAP-0084)")),
    responses(
        (status = 200, description = "Verification status", body = SourceVerificationResponse),
        (status = 400, description = "Invalid contract_id", body = ErrorEnvelope),
        (status = 404, description = "Contract not found, or no WASM", body = ErrorEnvelope),
        (status = 500, description = "Verification service unreadable or archive unusable", body = ErrorEnvelope),
    ),
)]
pub async fn get_verification(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Response {
    if state.runtime_enrichment.source_verification.is_none() {
        return unavailable(contract_id);
    }
    let l = match lookup(&state, &contract_id).await {
        Ok(found) => found,
        Err(resp) => return resp,
    };
    let (status, shown) = match reduce(&l) {
        Ok(reduced) => reduced,
        Err(resp) => return resp,
    };
    let mut source = None;
    let mut build = None;
    if let Some(mut r) = shown {
        // The recipe comes from the verifier whose archive is offered.
        match archive_of(&l.aggregate.source_code_verifications) {
            Ok(Some((publisher, published))) => {
                r = publisher.clone();
                source = Some(published);
            }
            Ok(None) => {}
            Err(e) => return archive_error(&contract_id, e),
        }
        build = Some(BuildRecipe {
            package: build_option(&r, "package"),
            manifest_path: build_option(&r, "manifest-path"),
            report_url: report_url(&r),
            image: r.bldimg,
            options: r.bldopt,
            arguments: r.bldarg,
            verified_at: r.processed_at,
        });
    }
    let mut resp = Json(SourceVerificationResponse {
        contract_id: l.contract_id.clone(),
        wasm_hash: Some(l.wasm_hash.clone()),
        status,
        checked_at: Some(l.aggregate.updated_at.clone()),
        verifiers: l
            .aggregate
            .source_code_verifications
            .iter()
            .map(|r| SourceVerifier {
                name: r.verifier.name.clone(),
                url: r.verifier.url.clone(),
                status: r.status.clone(),
            })
            .collect(),
        source,
        build,
    })
    .into_response();
    // A new verification can land at any time: the contract-detail tier.
    cache_control::attach(&mut resp, cache_control::MEDIUM);
    resp
}

/// A network without a verifier service says so on every contract page:
/// an explicit status, not a 404 that also means "no such contract", and
/// cached, because it changes only with a deploy. Neither the database nor
/// the service is asked.
fn unavailable(contract_id: String) -> Response {
    let contract_id = match path::contract_id(&contract_id) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let mut resp = Json(SourceVerificationResponse {
        contract_id,
        wasm_hash: None,
        status: VerificationStatus::Unavailable,
        checked_at: None,
        verifiers: Vec::new(),
        source: None,
        build: None,
    })
    .into_response();
    cache_control::attach(&mut resp, cache_control::LONG);
    resp
}

// ---------------------------------------------------------------------------
// GET /v1/contracts/{contract_id}/verified-source
// ---------------------------------------------------------------------------

/// The shown record's source archive, downloaded from the verifier's own
/// host, checked against its published sha256 and unpacked.
///
/// Only for `verified`: source that did not reproduce the on-chain code
/// (`mismatched`, `disputed`) is not served, and the page shows none of it.
/// 404 for every other status and when no source is published.
#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/verified-source",
    tag = "contracts",
    params(("contract_id" = String, Path, description = "Contract StrKey (C…, 56 chars), or the same contract with a sub-account id (W…, 69 chars, CAP-0084)")),
    responses(
        (status = 200, description = "Unpacked source archive", body = VerifiedSourceResponse),
        (status = 400, description = "Invalid contract_id", body = ErrorEnvelope),
        (status = 404, description = "No verifier service, contract or WASM, or no published source", body = ErrorEnvelope),
        (status = 500, description = "Verification service unreadable or archive failed its checks", body = ErrorEnvelope),
    ),
)]
pub async fn get_verified_source(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Response {
    let l = match lookup(&state, &contract_id).await {
        Ok(found) => found,
        Err(resp) => return resp,
    };
    match reduce(&l) {
        Ok((VerificationStatus::Verified, Some(_))) => {}
        Ok(_) => return errors::not_found("no verified published source for this contract"),
        Err(resp) => return resp,
    }
    let published = match archive_of(&l.aggregate.source_code_verifications) {
        Ok(Some((_, published))) => published,
        Ok(None) => return errors::not_found("no published source for this contract"),
        Err(e) => return archive_error(&contract_id, e),
    };
    let files = match fetch_files(&l.fetcher, &published).await {
        Ok(files) => files,
        Err(e) => return archive_error(&contract_id, e),
    };
    let mut body = VerifiedSourceResponse {
        contract_id: l.contract_id.clone(),
        wasm_hash: l.wasm_hash.clone(),
        archive_sha256: published.sha256,
        files: Vec::new(),
    };
    for f in files {
        body.files.push(VerifiedSourceFile {
            path: f.path,
            size: f.size,
            content: f.text,
        });
    }
    files_response(&contract_id, &body)
}

/// The files as a JSON response, refused when it would not fit a Lambda
/// response — JSON escaping can grow text past the unpacked cap, and past
/// the limit the runtime drops the response with an opaque 502.
fn files_response(contract_id: &str, body: &VerifiedSourceResponse) -> Response {
    let json = match serde_json::to_vec(body) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("verified source response did not serialise for {contract_id}: {e}");
            return errors::internal_error(
                errors::SOURCE_ARCHIVE_INVALID,
                "response encoding failed",
            );
        }
    };
    if !fits_lambda_response(&json) {
        return archive_error(contract_id, ArchiveError::UnpackCapExceeded);
    }
    let mut resp = (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response();
    // The archive is content-addressed, but the contract can move to other
    // code: the `/decompiled` tier.
    cache_control::attach(&mut resp, cache_control::LONG);
    resp
}

#[cfg(test)]
mod tests;
