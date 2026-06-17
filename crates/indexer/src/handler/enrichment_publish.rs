//! Type-1 enrichment SQS produce path (task 0191; SEP-1 `name`
//! writeback added in 0195 §2a; NFT `token_uri` kind added in 0195 §2d).
//!
//! ## Datasource: ClickHouse (task 0231, step 6)
//!
//! The producer-side dedup lookup — "which of the batch's freshly-extracted
//! keys lack an enrichment row yet" — runs against ClickHouse (PG is frozen
//! post-0241). It is a single per-batch anti-join against `*_enrichment`; see
//! [`enrichment_shared::enrich_and_persist::filter`]. The SQS wire carries the
//! composite key the worker decodes
//! ([`enrichment_shared::enrich_and_persist::EnrichmentMessage`]):
//! `(asset_type, asset_code, issuer_id, contract_id)` for SEP-1,
//! `(contract_id, token_id)` for NFTs.
//!
//! * **SEP-1 assets — LIVE.** [`Publisher::publish_for_extracted_assets`]
//!   derives the classic/SAC candidate keys, filters them through the CH
//!   anti-join (fail-open), and publishes only the misses.
//! * **NFT `token_uri` — LIVE.** [`Publisher::publish_for_minted_nfts`] derives
//!   the `(contract_id, token_id)` keys of the batch's freshly-minted NFTs,
//!   filters them through the CH anti-join (fail-open), and publishes the
//!   misses. Non-mint re-appearances are skipped at the call site (mint is the
//!   token_uri-set event); a prior-mint NFT still missing enrichment is the
//!   backfill's responsibility.
//!
//! The filter is an optimisation, not a correctness gate: the worker INSERT is
//! idempotent (`ReplacingMergeTree(version)`), so any race only costs a
//! redundant fetch, never a missing row.

use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sqs::types::SendMessageBatchRequestEntry;
use clickhouse::Client as ChClient;
use db_clickhouse::persist::ids;
use enrichment_shared::enrich_and_persist::{AssetKey, EnrichmentMessage, NftKey, filter};
use tracing::{debug, error, info, warn};
use xdr_parser::types::{ExtractedAsset, ExtractedNft};

const ENRICHMENT_QUEUE_URL_ENV: &str = "ENRICHMENT_QUEUE_URL";

/// SQS publisher for type-1 enrichment messages. Cheap to clone (both the SQS
/// and ClickHouse clients are `Arc`-pooled internally).
#[derive(Clone)]
pub struct Publisher {
    client: SqsClient,
    queue_url: String,
    /// Read-only — the producer-side `*_enrichment` anti-join (dedup filter).
    ch_client: ChClient,
}

impl Publisher {
    /// Read `ENRICHMENT_QUEUE_URL` from the environment and build a publisher
    /// over the given SQS + ClickHouse clients.
    pub fn from_env(client: SqsClient, ch_client: ChClient) -> Result<Self, String> {
        let url = std::env::var(ENRICHMENT_QUEUE_URL_ENV)
            .map_err(|_| format!("{ENRICHMENT_QUEUE_URL_ENV} must be set"))?;
        if url.is_empty() {
            return Err(format!("{ENRICHMENT_QUEUE_URL_ENV} must not be empty"));
        }
        info!(
            queue_url = %url,
            "enrichment SQS publisher initialised (sep1 assets + nft token_uri → CH filter live)"
        );
        Ok(Self {
            client,
            queue_url: url,
            ch_client,
        })
    }

    /// Publish SEP-1 enrichment messages for the batch's classic/SAC assets
    /// that have no `asset_enrichment` row yet. Native (0) / soroban (3) assets
    /// carry no SEP-1 enrichment and are skipped (Option C — their names come
    /// from `soroban_contracts` / a constant).
    ///
    /// Fail-open: a CH filter error publishes the full candidate set rather
    /// than drop enrichment — over-fetch is absorbed by the idempotent worker.
    pub async fn publish_for_extracted_assets(&self, extracted: &[ExtractedAsset]) {
        let candidates = sep1_candidate_keys(extracted);
        if candidates.is_empty() {
            return;
        }
        let to_publish = match filter::unenriched_asset_keys(&self.ch_client, &candidates).await {
            Ok(missing) => missing,
            Err(e) => {
                warn!(
                    error = %e,
                    candidates = candidates.len(),
                    "enrichment dedup filter failed; publishing all candidates (fail-open)"
                );
                candidates
            }
        };
        if to_publish.is_empty() {
            debug!("all batch assets already enriched; nothing to publish");
            return;
        }
        publish_sep1_assets_messages(&self.client, &self.queue_url, &to_publish).await;
    }

    /// Publish NFT `token_uri` enrichment messages for the batch's freshly-minted
    /// NFTs that have no `nft_enrichment` row yet. The call site passes only
    /// minted NFTs (mint = the token_uri-set event); non-mint re-appearances are
    /// already enriched, or are the backfill's job.
    ///
    /// Fail-open, same as the asset path.
    pub async fn publish_for_minted_nfts(&self, minted: &[ExtractedNft]) {
        let candidates = nft_candidate_keys(minted);
        if candidates.is_empty() {
            return;
        }
        let to_publish = match filter::unenriched_nft_keys(&self.ch_client, &candidates).await {
            Ok(missing) => missing,
            Err(e) => {
                warn!(
                    error = %e,
                    candidates = candidates.len(),
                    "nft enrichment dedup filter failed; publishing all candidates (fail-open)"
                );
                candidates
            }
        };
        if to_publish.is_empty() {
            debug!("all minted nfts already enriched; nothing to publish");
            return;
        }
        publish_nft_token_uri_messages(&self.client, &self.queue_url, &to_publish).await;
    }
}

/// Derive the classic/SAC `AssetKey`s from a batch's extracted assets,
/// mirroring the `assets`-table identity in `db_clickhouse::persist::stage`
/// (so the keys match the side-table rows the worker writes). Not deduped —
/// `filter::unenriched_asset_keys` collapses duplicates before the query.
fn sep1_candidate_keys(extracted: &[ExtractedAsset]) -> Vec<AssetKey> {
    extracted
        .iter()
        .filter_map(|t| {
            let asset_type = t.asset_type as i16;
            // Only classic credit (1) + SAC (2) carry SEP-1 enrichment.
            if asset_type != 1 && asset_type != 2 {
                return None;
            }
            Some(AssetKey {
                asset_type,
                asset_code: t.asset_code.clone().unwrap_or_default(),
                issuer_id: t
                    .issuer_address
                    .as_deref()
                    .map(ids::account_id)
                    .unwrap_or(0),
                contract_id: t.contract_id.as_deref().map(ids::contract_id).unwrap_or(0),
            })
        })
        .collect()
}

/// Derive the `NftKey`s from a batch's minted NFTs, mirroring the `nfts`-table
/// identity in `db_clickhouse::persist::stage` (so the keys match the side-table
/// rows the worker writes). Not deduped — `filter::unenriched_nft_keys`
/// collapses duplicates before the query.
fn nft_candidate_keys(minted: &[ExtractedNft]) -> Vec<NftKey> {
    minted
        .iter()
        .map(|n| NftKey {
            contract_id: ids::contract_id(&n.contract_id),
            token_id: n.token_id.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Wire-format helpers
// ---------------------------------------------------------------------------
//
// Both helpers are live: `publish_sep1_assets_messages` is called by
// `publish_for_extracted_assets`, `publish_nft_token_uri_messages` by
// `publish_for_minted_nfts`. Each builds its per-kind entries then hands the
// chunk to the shared `send_one_batch`.

/// Send one ≤10-entry batch via SQS `SendMessageBatch` (the API's hard cap),
/// logging partial failures. `kind` tags the log lines. Empty batch → no-op.
async fn send_one_batch(
    client: &SqsClient,
    queue_url: &str,
    entries: Vec<SendMessageBatchRequestEntry>,
    kind: &str,
) {
    if entries.is_empty() {
        return;
    }
    let resp = client
        .send_message_batch()
        .queue_url(queue_url)
        .set_entries(Some(entries))
        .send()
        .await;
    match resp {
        Ok(out) => {
            let failed = out.failed.len();
            if failed > 0 {
                let failures: Vec<String> = out
                    .failed
                    .iter()
                    .map(|f| {
                        format!(
                            "{}:{}({})",
                            f.id,
                            f.code,
                            if f.sender_fault { "sender" } else { "receiver" }
                        )
                    })
                    .collect();
                error!(
                    failed,
                    failures = ?failures,
                    kind,
                    "SQS send_message_batch reported partial failures"
                );
            }
            debug!(
                successful = out.successful.len(),
                failed, kind, "SQS batch published"
            );
        }
        Err(e) => error!(error = %e, kind, "SQS send_message_batch failed"),
    }
}

pub(crate) async fn publish_nft_token_uri_messages(
    client: &SqsClient,
    queue_url: &str,
    keys: &[NftKey],
) {
    for chunk in keys.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, key) in chunk.iter().enumerate() {
            // Encode via the shared `EnrichmentMessage` so the wire cannot drift
            // from the worker's decoder (single source of truth, message.rs).
            let body = match serde_json::to_string(&EnrichmentMessage::NftTokenUri(key.clone())) {
                Ok(body) => body,
                Err(e) => {
                    warn!(error = %e, key = %key, "skipping unencodable enrichment msg");
                    continue;
                }
            };
            debug!(kind = "nft_token_uri", key = %key, "publishing enrichment msg");
            // `idx` guarantees Id uniqueness within the batch; the full key
            // makes a failed entry (SQS reports failures by Id) traceable.
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!("nft-{idx}-{key}"))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(error = %e, key = %key, "skipping malformed SQS entry"),
            }
        }
        send_one_batch(client, queue_url, entries, "nft_token_uri").await;
    }
}

pub(crate) async fn publish_sep1_assets_messages(
    client: &SqsClient,
    queue_url: &str,
    keys: &[AssetKey],
) {
    for chunk in keys.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, key) in chunk.iter().enumerate() {
            // Encode via the shared `EnrichmentMessage` so the wire cannot drift
            // from the worker's decoder (single source of truth, message.rs).
            let body = match serde_json::to_string(&EnrichmentMessage::Sep1Assets(key.clone())) {
                Ok(body) => body,
                Err(e) => {
                    warn!(error = %e, key = %key, "skipping unencodable enrichment msg");
                    continue;
                }
            };
            debug!(kind = "sep1_assets", key = %key, "publishing enrichment msg");
            // `idx` guarantees Id uniqueness within the batch; the full
            // composite key makes a failed entry (SQS reports failures by Id)
            // traceable to its source row.
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!("msg-{idx}-{key}"))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(error = %e, key = %key, "skipping malformed SQS entry"),
            }
        }
        send_one_batch(client, queue_url, entries, "sep1_assets").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::TokenAssetType;

    fn asset(
        at: TokenAssetType,
        code: Option<&str>,
        issuer: Option<&str>,
        contract: Option<&str>,
    ) -> ExtractedAsset {
        ExtractedAsset {
            asset_type: at,
            asset_code: code.map(String::from),
            issuer_address: issuer.map(String::from),
            contract_id: contract.map(String::from),
            name: None,
            total_supply: None,
            holder_count: None,
        }
    }

    #[test]
    fn candidate_keys_keep_classic_sac_skip_native_soroban() {
        let extracted = vec![
            asset(
                TokenAssetType::ClassicCredit,
                Some("USDC"),
                Some("GISSUER1"),
                None,
            ),
            asset(
                TokenAssetType::Sac,
                Some("SAC"),
                Some("GISSUER2"),
                Some("CCONTRACT"),
            ),
            asset(TokenAssetType::Native, None, None, None), // skipped (0)
            asset(TokenAssetType::Soroban, None, None, Some("CSOROBAN")), // skipped (3)
        ];

        let keys = sep1_candidate_keys(&extracted);

        // classic + sac = two; native/soroban excluded. Dedup is the filter's job.
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&AssetKey {
            asset_type: 1,
            asset_code: "USDC".into(),
            issuer_id: ids::account_id("GISSUER1"),
            contract_id: 0,
        }));
        assert!(keys.contains(&AssetKey {
            asset_type: 2,
            asset_code: "SAC".into(),
            issuer_id: ids::account_id("GISSUER2"),
            contract_id: ids::contract_id("CCONTRACT"),
        }));
    }

    #[test]
    fn candidate_keys_empty_when_no_classic_or_sac() {
        let extracted = vec![
            asset(TokenAssetType::Native, None, None, None),
            asset(TokenAssetType::Soroban, None, None, Some("C")),
        ];
        assert!(sep1_candidate_keys(&extracted).is_empty());
    }

    fn nft(contract: &str, token: &str) -> ExtractedNft {
        ExtractedNft {
            contract_id: contract.into(),
            token_id: token.into(),
            collection_name: None,
            owner_account: None,
            name: None,
            media_url: None,
            minted_at_ledger: Some(42),
            last_seen_ledger: 42,
            created_at: 0,
        }
    }

    #[test]
    fn nft_candidate_keys_derive() {
        let minted = vec![
            nft("CNFT1", "1"),
            nft("CNFT1", "2"), // same contract, different token
            nft("CNFT2", "1"), // different contract, same token
        ];

        let keys = nft_candidate_keys(&minted);

        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&NftKey {
            contract_id: ids::contract_id("CNFT1"),
            token_id: "1".into(),
        }));
        assert!(keys.contains(&NftKey {
            contract_id: ids::contract_id("CNFT1"),
            token_id: "2".into(),
        }));
        assert!(keys.contains(&NftKey {
            contract_id: ids::contract_id("CNFT2"),
            token_id: "1".into(),
        }));
    }
}
