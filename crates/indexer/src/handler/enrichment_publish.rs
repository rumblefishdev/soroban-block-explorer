//! Type-1 enrichment SQS produce path (task 0191; SEP-1 `name`
//! writeback added in 0195 §2a; NFT `token_uri` kind added in 0195 §2d).
//!
//! ## Stub status (task 0241 — hard swap PG → CH)
//!
//! After the indexer cut over to ClickHouse, the pre-publish lookups
//! that select which rows need enrichment can no longer query Postgres:
//! PG is frozen at the cutover ledger and any row written by the indexer
//! post-cutover lives in CH only.
//!
//! The paired CH-aware rewrite has now landed for the **write** side
//! (task 0231): the `enrichment-worker` + the shared `enrich_*` functions
//! write the ClickHouse side tables (`asset_enrichment` / `nft_enrichment`,
//! ADR 0048), keyed by the composite identity (no CH surrogate `id`), and
//! the SQS wire carries that composite key — see
//! `enrichment_shared::enrich_and_persist::EnrichmentMessage` and the
//! updated wire helpers below (`(asset_type, asset_code, issuer_id,
//! contract_id)` for SEP-1, `(contract_id, token_id)` for NFTs).
//!
//! What is still **stubbed** is only the producer-side *lookup*: selecting
//! which keys lack a side-table row (a CH `NOT IN *_enrichment` query over
//! the freshly-extracted batch). Until that lands:
//!
//! * [`Publisher::from_env`] still reads `ENRICHMENT_QUEUE_URL` and
//!   validates it — Lambda cold start still fails fast on misconfig
//!   so CDK stays correct and the SQS / IAM wiring keeps working.
//! * [`Publisher::publish_for_extracted_assets`] and
//!   [`Publisher::publish_for_minted_nfts`] are no-ops — they emit a
//!   warn-once trace per process so operators see the stub is active.
//!
//! The wire-format helpers and the SQS plumbing stay intact so the
//! re-enablement diff is a swap of the lookup implementation, not a
//! rebuild from scratch.

use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sqs::types::SendMessageBatchRequestEntry;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, error, info, warn};
use xdr_parser::types::ExtractedAsset;

const ENRICHMENT_QUEUE_URL_ENV: &str = "ENRICHMENT_QUEUE_URL";

/// One-shot guards so each warn-once line lands in CloudWatch exactly
/// once per Lambda execution environment instead of on every batch.
static WARNED_ASSETS_STUB: AtomicBool = AtomicBool::new(false);
static WARNED_NFTS_STUB: AtomicBool = AtomicBool::new(false);

/// SQS publisher for type-1 enrichment messages. Cheap to clone.
#[derive(Clone)]
pub struct Publisher {
    // Both fields are retained for the stubbed wire-format helpers
    // (`publish_sep1_assets_messages` / `publish_nft_token_uri_messages`);
    // those helpers are `#[allow(dead_code)]` until the CH-aware lookup
    // lands, so dead-code analysis flags the fields without this guard.
    #[allow(dead_code)]
    client: SqsClient,
    #[allow(dead_code)]
    queue_url: String,
}

impl Publisher {
    /// Read `ENRICHMENT_QUEUE_URL` from the environment and build a
    /// publisher.
    pub fn from_env(client: SqsClient) -> Result<Self, String> {
        let url = std::env::var(ENRICHMENT_QUEUE_URL_ENV)
            .map_err(|_| format!("{ENRICHMENT_QUEUE_URL_ENV} must be set"))?;
        if url.is_empty() {
            return Err(format!("{ENRICHMENT_QUEUE_URL_ENV} must not be empty"));
        }
        info!(queue_url = %url, "enrichment SQS publisher initialised (stub mode — see 0241)");
        Ok(Self {
            client,
            queue_url: url,
        })
    }

    /// Stubbed — see module-level docs. The `_extracted` slice is the
    /// parser's per-ledger `ExtractedAsset` list; once the CH-aware
    /// lookup lands it filters this set against CH-side "missing
    /// icon_url / name" predicates. Today we just log-once and
    /// return.
    pub async fn publish_for_extracted_assets(&self, _extracted: &[ExtractedAsset]) {
        if !WARNED_ASSETS_STUB.swap(true, Ordering::Relaxed) {
            warn!(
                "enrichment publish (sep1_assets) is stubbed post-CH cutover; \
                 see crates/indexer/src/handler/enrichment_publish.rs for re-enablement"
            );
        }
    }

    /// Stubbed counterpart for the NFT mint hook (task 0195 §2d).
    pub async fn publish_for_minted_nfts(&self, _ledger_sequences: &[u32]) {
        if !WARNED_NFTS_STUB.swap(true, Ordering::Relaxed) {
            warn!(
                "enrichment publish (nft_token_uri) is stubbed post-CH cutover; \
                 see crates/indexer/src/handler/enrichment_publish.rs for re-enablement"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Wire-format helpers
// ---------------------------------------------------------------------------
//
// Retained as `pub(crate)` so the re-enablement diff is a small swap of
// the lookup body in `Publisher::publish_for_*` — not a from-scratch
// rebuild of the SQS batching logic. The functions remain
// implementation-correct and exercised by future tests; today they are
// not called.

#[allow(dead_code)]
pub(crate) async fn publish_nft_token_uri_messages(
    client: &SqsClient,
    queue_url: &str,
    keys: &[(i64, String)],
) {
    for chunk in keys.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, (contract_id, token_id)) in chunk.iter().enumerate() {
            let body = serde_json::json!({
                "kind": "nft_token_uri",
                "contract_id": contract_id,
                "token_id": token_id,
            })
            .to_string();
            debug!(
                kind = "nft_token_uri",
                contract_id, token_id, "publishing enrichment msg"
            );
            // `idx` guarantees Id uniqueness within the batch; the full key
            // makes a failed entry (SQS reports failures by Id) traceable.
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!("nft-{idx}-{contract_id}-{token_id}"))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(
                    error = %e,
                    contract_id,
                    token_id,
                    "skipping malformed SQS entry"
                ),
            }
        }
        if entries.is_empty() {
            continue;
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
                        "SQS send_message_batch reported partial failures (nft_token_uri)",
                    );
                }
                debug!(
                    successful = out.successful.len(),
                    failed, "SQS batch published (nft_token_uri)"
                );
            }
            Err(e) => error!(error = %e, "SQS send_message_batch failed (nft_token_uri)"),
        }
    }
}

#[allow(dead_code)]
pub(crate) async fn publish_sep1_assets_messages(
    client: &SqsClient,
    queue_url: &str,
    keys: &[(i16, String, i64, i64)],
) {
    for chunk in keys.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, (asset_type, asset_code, issuer_id, contract_id)) in chunk.iter().enumerate() {
            let body = serde_json::json!({
                "kind": "sep1_assets",
                "asset_type": asset_type,
                "asset_code": asset_code,
                "issuer_id": issuer_id,
                "contract_id": contract_id,
            })
            .to_string();
            debug!(
                kind = "sep1_assets",
                asset_type, asset_code, issuer_id, contract_id, "publishing enrichment msg"
            );
            // `idx` guarantees Id uniqueness within the batch; the full
            // composite key makes a failed entry (SQS reports failures by Id)
            // traceable to its source row.
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!(
                    "msg-{idx}-{asset_type}-{asset_code}-{issuer_id}-{contract_id}"
                ))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(
                    error = %e,
                    asset_type,
                    asset_code,
                    issuer_id,
                    contract_id,
                    "skipping malformed SQS entry"
                ),
            }
        }
        if entries.is_empty() {
            continue;
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
                        "SQS send_message_batch reported partial failures",
                    );
                }
                debug!(
                    successful = out.successful.len(),
                    failed, "SQS batch published"
                );
            }
            Err(e) => error!(error = %e, "SQS send_message_batch failed"),
        }
    }
}
