//! Per-CONTRACT `collection_name` backfill from the SEP-50 `name()` RPC
//! simulate (task 0340).
//!
//! The live path (`nft_token_uri::enrich_nft_token_uri`) fills
//! `collection_name` for every NEW row it writes; this unit of work repairs
//! the EXISTING rows — the prod cohort enriched before 0340, where `name` /
//! `media_url` are ~84% real but `collection_name` is 0%. Those rows do NOT
//! match the runner's `--retry-sentinels` drain (it requires ALL columns
//! empty, by design — re-fetching them per token would also risk clobbering
//! real values on a flaky upstream), so the backfill walks CONTRACTS instead:
//! one `name()` RPC per contract, then one INSERT-SELECT that stamps the name
//! onto the contract's rows while PRESERVING each row's `name` / `media_url`
//! (RMT whole-row replace — see `persist::rewrite_nft_collection_name`).
//!
//! Failure model mirrors the other `enrich_*` units: transient RPC fault →
//! `EnrichError::Transient` (the runner reports it; the contract retries next
//! run); a permanent "no usable name()" → `Sentinel` outcome with NO write
//! (the rows already carry the `''` sentinel — rewriting them would only bump
//! `version` for nothing).

use clickhouse::{Client, Row};
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use super::persist::rewrite_nft_collection_name;
use super::{EnrichError, EnrichOutcome};
use crate::nft_token_uri::NftTokenUriFetcher;

/// Contract StrKey looked up by the `soroban_contracts.id` FK (same shape as
/// the lookup in `nft_token_uri`).
#[derive(Row, Deserialize)]
struct StrkeyLookup {
    contract_strkey: Option<String>,
}

#[instrument(skip(client, fetcher), fields(contract_id = contract_id))]
pub async fn backfill_contract_collection_name(
    client: &Client,
    contract_id: i64,
    fetcher: &NftTokenUriFetcher,
) -> Result<EnrichOutcome, EnrichError> {
    let lookup = client
        .query(
            "SELECT nullIf(contract_id, '') AS contract_strkey \
             FROM soroban_contracts FINAL WHERE id = ? LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<StrkeyLookup>()
        .await?;

    let Some(contract_strkey) = lookup.and_then(|l| l.contract_strkey) else {
        warn!(reason = "contract_strkey_not_found", "skipping contract");
        return Ok(EnrichOutcome::Sentinel);
    };

    match fetcher.resolve_collection_name(&contract_strkey).await {
        Ok(Some(name)) => {
            rewrite_nft_collection_name(client, contract_id, &name).await?;
            debug!(collection_name = %name, "collection_name stamped onto contract rows");
            Ok(EnrichOutcome::Real)
        }
        // Permanent "no usable name()" — rows already sentinel; nothing to write.
        Ok(None) => {
            debug!(reason = "name_no_value", "no write");
            Ok(EnrichOutcome::Sentinel)
        }
        // Every Err from resolve_collection_name is transient by contract
        // (permanent fails fold to Ok(None) inside the fetcher).
        Err(arc_err) => {
            warn!(reason = "name_transient", error = %arc_err, "retry candidate (no write)");
            Err(EnrichError::Transient(arc_err.to_string()))
        }
    }
}
