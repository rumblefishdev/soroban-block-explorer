//! The single write surface for off-chain enrichment.
//!
//! Every value AND every `''` sentinel written by either entry point — the live
//! SQS-driven `enrichment-worker` Lambda and the `backfill-enrichment-runner` —
//! funnels through these two functions (both `enrich_*` paths call them).
//! Centralised here so the production write blast-radius is one auditable file:
//! it INSERTs into `asset_enrichment` / `nft_enrichment` ONLY — append-only
//! `ReplacingMergeTree` (latest-wins by `version`), never an UPDATE/DELETE, and
//! never the indexer-owned `assets` / `nfts` / `accounts` / `soroban_contracts`
//! tables. The raw column INSERT lives one layer down in
//! `db_clickhouse::persist::enrichment`; these wrappers build the typed row and
//! classify the [`EnrichOutcome`] (Real vs `''` Sentinel).

use clickhouse::Client;
use db_clickhouse::persist::enrichment::{insert_asset_enrichment, insert_nft_enrichment};
use db_clickhouse::persist::rows::{AssetEnrichmentRow, NftEnrichmentRow};

use super::{AssetKey, EnrichError, EnrichOutcome, NftKey, now_ms};

/// Build + INSERT one `asset_enrichment` row, returning whether a real value or
/// only the `''` sentinel landed. `''` is stored as-is (read-neutralised with
/// `NULLIF`). Parallel to [`insert_nft`]: `(client, &key, …resolved values…)`.
pub(crate) async fn insert_asset(
    client: &Client,
    key: &AssetKey,
    icon_url: String,
    name: Option<String>,
) -> Result<EnrichOutcome, EnrichError> {
    // Real iff a non-sentinel value landed in either column.
    let outcome = if !icon_url.is_empty() || name.as_deref().is_some_and(|n| !n.is_empty()) {
        EnrichOutcome::Real
    } else {
        EnrichOutcome::Sentinel
    };
    let row = AssetEnrichmentRow {
        asset_type: key.asset_type,
        asset_code: key.asset_code.clone(),
        issuer_id: key.issuer_id,
        contract_id: key.contract_id,
        icon_url: Some(icon_url),
        name,
        version: now_ms(),
    };
    insert_asset_enrichment(client, std::slice::from_ref(&row)).await?;
    Ok(outcome)
}

/// Build + INSERT one `nft_enrichment` row, returning whether a real value or
/// only the `''` sentinel landed. `''` is stored as-is (read-neutralised with
/// `NULLIF`). Parallel to [`insert_asset`]: `(client, &key, …resolved values…)`.
/// The three value columns are owned `String`s (NFT has no nullable name, unlike
/// the asset's `Option<String>`).
pub(crate) async fn insert_nft(
    client: &Client,
    key: &NftKey,
    name: String,
    media_url: String,
    collection_name: String,
) -> Result<EnrichOutcome, EnrichError> {
    // Real iff any of the three columns landed a non-sentinel value.
    let outcome = if !name.is_empty() || !media_url.is_empty() || !collection_name.is_empty() {
        EnrichOutcome::Real
    } else {
        EnrichOutcome::Sentinel
    };
    let row = NftEnrichmentRow {
        contract_id: key.contract_id,
        token_id: key.token_id.clone(),
        name: Some(name),
        media_url: Some(media_url),
        collection_name: Some(collection_name),
        version: now_ms(),
    };
    insert_nft_enrichment(client, std::slice::from_ref(&row)).await?;
    Ok(outcome)
}
