//! Type-1 enrichment SQS produce path (task 0191; SEP-1 `name`
//! writeback added in 0195 §2a; NFT `token_uri` kind added in 0195 §2d).
//!
//! After a ledger's persistence transaction commits, the indexer
//! publishes SQS messages for every row that needs off-chain enrichment.
//! Two kinds today:
//!
//! - `sep1_assets` (SEP-1 issuer TOML; historical wire name was
//!   `"icon"` — see breaking-change note below) — worker fetches
//!   `https://{home_domain}/.well-known/stellar.toml` and writes
//!   `assets.icon_url` (all asset types) plus `assets.name`
//!   (ClassicCredit + SAC, `asset_type IN (1, 2)`).
//! - `nft_token_uri` — worker calls Soroban RPC `simulateTransaction`
//!   on the contract's `token_uri(token_id)` view, fetches the
//!   resulting URL (HTTP / IPFS gateway) and writes
//!   `nfts.{name, media_url, collection_name}`.
//!
//! Wire `kind` is `"sep1_assets"` (snake_case form of the Rust variant
//! `EnrichmentMessage::Sep1Assets`). The historical name was `"icon"`
//! — renamed in 0196 once the kind grew beyond `icon_url` to also
//! write `assets.name`. **Breaking change**: pre-rename in-flight SQS
//! messages and DLQ entries with `"kind":"icon"` will not deserialise
//! against the current worker; drain the DLQ before deploy if any are
//! present.
//!
//! ## Selection criteria
//!
//! After commit, query for asset rows that:
//!
//! - Match a `(code, issuer_strkey)` tuple or `contract_id` StrKey from
//!   the parser's `ExtractedAsset` slice for this ledger, AND
//! - Are missing at least one column the `sep1_assets` worker fills:
//!     - `icon_url IS NULL`, OR
//!     - `asset_type IN (1, 2) AND name IS NULL` — ClassicCredit + SAC
//!       rows whose human-readable `name` has not yet been resolved
//!       from the issuer's SEP-1 TOML (task 0195). Soroban-native
//!       (asset_type=3) is filled by the indexer (task 0156); native
//!       (asset_type=0) is out of scope.
//!
//! Both predicates use the same `''` sentinel pattern: a permanent
//! enrichment fail writes `''` (not NULL), so already-attempted assets
//! drop out of the predicate naturally. Transient failures retain NULL
//! and re-attempt via SQS retry → DLQ.
//!
//! This intentionally re-emits messages for *un-enriched but
//! pre-existing* asset rows that happened to be touched by this
//! ledger. The worker absorbs the cost of duplicates per the contract
//! in `enrichment_shared::enrich_and_persist::sep1_assets`. Once an asset is enriched it
//! drops out of this query naturally.
//!
//! ## Configuration
//!
//! `ENRICHMENT_QUEUE_URL` env var holds the SQS queue URL provisioned
//! by CDK. The indexer Lambda is a deploy-only artifact (CDK always
//! sets the variable), so the variable is **required** — a missing or
//! empty value fails Lambda cold start instead of silently disabling
//! the producer. CW `Init Errors` surfaces the misconfig immediately;
//! recovery is a fix-the-env-var redeploy. The trade-off accepted:
//! ingestion stops on enrichment misconfig (operator choice — explicit
//! signal preferred over partial availability).
//!
//! ## Failure model
//!
//! Publish failures are warn-logged and never propagated to the
//! handler. A dropped enrichment message is recoverable: a future
//! janitor (Future Work in 0191) re-emits stale rows, and the
//! operator-driven backfill (separate future task) drains
//! `WHERE icon_url IS NULL` directly. The persistence transaction has
//! already committed — fail-soft is correct here.

use aws_sdk_sqs::Client as SqsClient;
use aws_sdk_sqs::types::SendMessageBatchRequestEntry;
use sqlx::{PgPool, Row};
use tracing::{debug, error, info, instrument, warn};
use xdr_parser::types::ExtractedAsset;

const ENRICHMENT_QUEUE_URL_ENV: &str = "ENRICHMENT_QUEUE_URL";

/// SQS publisher for type-1 enrichment messages. Cheap to clone.
#[derive(Clone)]
pub struct Publisher {
    client: SqsClient,
    queue_url: String,
}

impl Publisher {
    /// Read `ENRICHMENT_QUEUE_URL` from the environment and build a
    /// publisher. Returns `Err` (string for the Lambda init error path)
    /// when the variable is missing or empty — the indexer Lambda is
    /// deploy-only and CDK always sets it, so a missing value is a
    /// misconfig that should fail cold start instead of silently
    /// disabling the producer.
    pub fn from_env(client: SqsClient) -> Result<Self, String> {
        let url = std::env::var(ENRICHMENT_QUEUE_URL_ENV)
            .map_err(|_| format!("{ENRICHMENT_QUEUE_URL_ENV} must be set"))?;
        if url.is_empty() {
            return Err(format!("{ENRICHMENT_QUEUE_URL_ENV} must not be empty"));
        }
        info!(queue_url = %url, "enrichment SQS publisher initialised");
        Ok(Self {
            client,
            queue_url: url,
        })
    }

    /// Look up un-enriched asset ids matching the parser's extracted
    /// assets and emit one `icon` SQS message per id.
    ///
    /// `extracted` is the parser's per-ledger `ExtractedAsset` slice.
    /// Empty slice short-circuits without touching the database.
    #[instrument(skip_all, fields(extracted = extracted.len()))]
    pub async fn publish_for_extracted_assets(&self, pool: &PgPool, extracted: &[ExtractedAsset]) {
        if extracted.is_empty() {
            return;
        }

        let asset_ids = match select_unenriched_asset_ids(pool, extracted).await {
            Ok(ids) => ids,
            Err(e) => {
                // ERROR (not WARN) so log-based alarms surface a sustained
                // outage. Indexer continues — persist_ledger has already
                // committed; the un-enriched rows are picked up by the
                // next ledger that touches them (still `WHERE icon_url
                // IS NULL`) or by a future janitor / backfill.
                error!(error = %e, "enrichment lookup failed; skipping SQS publish");
                return;
            }
        };

        if asset_ids.is_empty() {
            debug!("no un-enriched assets matched the extracted set; nothing to publish");
            return;
        }

        publish_sep1_assets_messages(&self.client, &self.queue_url, &asset_ids).await;
    }

    /// Insert-hook publisher for NFT mints (task 0195 §2d). Looks up
    /// `nfts.id` rows with `minted_at_ledger IN ($ledgers)` that still
    /// have `NULL` in any of `name` / `media_url` / `collection_name`
    /// and emits one `nft_token_uri` message per id.
    ///
    /// Insert-hook semantics: a freshly minted NFT row has all
    /// off-chain columns NULL. Once the worker writes either a real
    /// value or the `''` sentinel, the predicate is false and the row
    /// drops out of the query. Re-running the same ledger (idempotent
    /// retries) re-selects the same id but the worker UPDATE is
    /// idempotent so duplicate emissions are harmless.
    ///
    /// Bounded to the current batch's `minted_at_ledger`: an SQS publish
    /// failure leaks those nft_ids (no re-emission window since the
    /// mint ledger has passed). Outbox-style transactional emit is
    /// infra overkill for a block explorer; the 0196 enrichment
    /// backfill crate drains the gap directly.
    #[instrument(skip_all, fields(ledgers = ledger_sequences.len()))]
    pub async fn publish_for_minted_nfts(&self, pool: &PgPool, ledger_sequences: &[u32]) {
        if ledger_sequences.is_empty() {
            return;
        }
        let ledgers_i64: Vec<i64> = ledger_sequences.iter().map(|&l| i64::from(l)).collect();
        let nft_ids = match select_unenriched_nft_ids(pool, &ledgers_i64).await {
            Ok(ids) => ids,
            Err(e) => {
                error!(error = %e, "nft enrichment lookup failed; skipping SQS publish");
                return;
            }
        };
        if nft_ids.is_empty() {
            debug!("no un-enriched NFT mints in this batch; nothing to publish");
            return;
        }
        publish_nft_token_uri_messages(&self.client, &self.queue_url, &nft_ids).await;
    }
}

/// Find ids of asset rows whose `icon_url IS NULL` and which match a
/// `(code, issuer_strkey)` tuple or `contract_id` StrKey from the
/// extracted set. Empty extracted set → empty result.
async fn select_unenriched_asset_ids(
    pool: &PgPool,
    extracted: &[ExtractedAsset],
) -> Result<Vec<i32>, sqlx::Error> {
    let mut codes: Vec<String> = Vec::new();
    let mut issuers: Vec<String> = Vec::new();
    let mut contracts: Vec<String> = Vec::new();
    for ext in extracted {
        if let (Some(code), Some(issuer)) = (&ext.asset_code, &ext.issuer_address) {
            codes.push(code.clone());
            issuers.push(issuer.clone());
        }
        if let Some(contract) = &ext.contract_id {
            contracts.push(contract.clone());
        }
    }
    if codes.is_empty() && contracts.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT DISTINCT a.id
        FROM assets a
        LEFT JOIN accounts iss          ON iss.id = a.issuer_id
        LEFT JOIN soroban_contracts sc  ON sc.id = a.contract_id
        WHERE (
                a.icon_url IS NULL
                OR (a.asset_type IN (1, 2) AND a.name IS NULL)
              )
          AND (
                -- classic_credit / sac match by (code, issuer_strkey) tuple
                (a.asset_code, iss.account_id) IN (
                    SELECT * FROM UNNEST($1::VARCHAR[], $2::VARCHAR[])
                )
                -- soroban / sac match by contract StrKey
                OR sc.contract_id = ANY($3::VARCHAR[])
              )
        "#,
    )
    .bind(&codes)
    .bind(&issuers)
    .bind(&contracts)
    .fetch_all(pool)
    .await?;

    // Propagate decode errors instead of silently dropping rows. A schema
    // drift on `assets.id` (column rename, type change, unexpected NULL)
    // would otherwise hide enrichment misses behind a clean log.
    rows.into_iter()
        .map(|r| r.try_get::<i32, _>("id"))
        .collect()
}

/// Insert-hook predicate for NFT mints — any of the three target
/// columns being NULL is the "not yet enriched" signal. The worker
/// writes either a real value or the `''` sentinel on every code path,
/// so a fully-processed row (real or sentinel everywhere) naturally
/// drops out of this query.
///
/// Why test all three (matching `sep1_assets` / §2a's `WHERE icon_url
/// IS NULL OR (... AND name IS NULL)`): the worker's UPDATE uses
/// `COALESCE(NULLIF($n, ''), col, $n)` priority `real > sentinel >
/// NULL`. If a partial write lands (e.g. sentinel run lost a column to
/// the `COALESCE`-into-existing branch on a retry that found it
/// already populated), we still want to re-emit so a follow-up fetch
/// can fill the remaining NULL. Checking only `name IS NULL` would
/// miss those rows.
async fn select_unenriched_nft_ids(
    pool: &PgPool,
    ledger_sequences: &[i64],
) -> Result<Vec<i32>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id
          FROM nfts
         WHERE minted_at_ledger = ANY($1::BIGINT[])
           AND (name IS NULL OR media_url IS NULL OR collection_name IS NULL)
        "#,
    )
    .bind(ledger_sequences)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| r.try_get::<i32, _>("id"))
        .collect()
}

async fn publish_nft_token_uri_messages(client: &SqsClient, queue_url: &str, nft_ids: &[i32]) {
    for chunk in nft_ids.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, id) in chunk.iter().enumerate() {
            let body = serde_json::json!({ "kind": "nft_token_uri", "nft_id": id }).to_string();
            debug!(
                kind = "nft_token_uri",
                nft_id = id,
                "publishing enrichment msg"
            );
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!("nft-{idx}-{id}"))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(error = %e, nft_id = id, "skipping malformed SQS entry"),
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

async fn publish_sep1_assets_messages(client: &SqsClient, queue_url: &str, asset_ids: &[i32]) {
    // SendMessageBatch caps at 10 messages per request.
    for chunk in asset_ids.chunks(10) {
        let mut entries = Vec::with_capacity(chunk.len());
        for (idx, id) in chunk.iter().enumerate() {
            // Build the JSON body via serde so future kinds with
            // string fields can't accidentally introduce injection.
            let body = serde_json::json!({ "kind": "sep1_assets", "asset_id": id }).to_string();
            debug!(
                kind = "sep1_assets",
                asset_id = id,
                "publishing enrichment msg"
            );
            let entry = SendMessageBatchRequestEntry::builder()
                .id(format!("msg-{idx}-{id}"))
                .message_body(body)
                .build();
            match entry {
                Ok(entry) => entries.push(entry),
                Err(e) => warn!(error = %e, asset_id = id, "skipping malformed SQS entry"),
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
                    // ERROR — partial-batch failure leaks msgs (those entries
                    // never reach the queue). Surface so a sustained pattern
                    // is alarm-able. Each failed entry's id + sender_fault +
                    // code is included for triage.
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
            // ERROR — full-batch failure (network, throttle, IAM, etc.).
            // Same recovery story as the lookup failure above: msgs are
            // lost for this ledger, recovered later via `WHERE icon_url
            // IS NULL` re-emission or backfill.
            Err(e) => error!(error = %e, "SQS send_message_batch failed"),
        }
    }
}
