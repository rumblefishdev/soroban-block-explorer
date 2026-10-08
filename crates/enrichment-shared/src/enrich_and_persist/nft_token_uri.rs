//! `nft_enrichment` side-table fill from per-token `token_uri()` JSON
//! metadata (task 0195 §2d / ADR 0050) + the contract-level SEP-50
//! `name()` for `collection_name` (task 0340).
//!
//! Writes `(name, media_url, collection_name)` into the `nft_enrichment`
//! side table — never the indexer-owned `nfts` table. These three are
//! **enrichment-only** (the indexer always writes `None`: a Stellar NFT
//! mint event carries no metadata), so the read path uses the side table
//! directly, no COALESCE to the indexer (task 0231). `name`/`media_url`
//! come from the token_uri JSON; `collection_name` from the per-contract
//! `name()` RPC simulate (no real-world contract emits a JSON
//! `"collection"` field — that source measured 0/68 on prod, task 0340).
//!
//! ### Failure model — soft-fail downstream of fetcher
//!
//! - transient errors (Http 5xx / connect / timeout, SorobanRpc) bubble as
//!   `EnrichError::Transient` so SQS retries;
//! - permanent errors (4xx, malformed JSON, unsafe scheme, XDR codec) and
//!   `Ok(None)` write the `''` sentinel, so the row records "fetch
//!   attempted, no value" and the candidate query (`NOT IN nft_enrichment`)
//!   skips the key on the next pass;
//! - the shared `is_safe_https_url` replaces a non-`https://` `image` with
//!   the sentinel — defence in depth against a smuggled scheme.
//!
//! The side table is `ReplacingMergeTree(version)`: every write is an
//! INSERT with `version = now_ms`, latest-wins. A later DLQ replay /
//! backfill upgrades a sentinel by inserting a newer-version row; the read
//! path neutralises `''` with `NULLIF`.
//!
//! ### Two `token_uri` response conventions (handled by the fetcher)
//!
//! - `application/json` — standard NFT metadata. Parse → `name`,
//!   `image` → `media_url`, `collection`.
//! - `image/*` — direct-image convention; the URI itself is the image,
//!   the fetcher synthesises `{ "image": "<url>" }`.

use clickhouse::{Client, Row};
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, instrument, warn};

use super::persist::insert_nft;
use super::{EnrichError, EnrichOutcome, NftKey};
use crate::nft_token_uri::NftTokenUriFetcher;
use crate::nft_token_uri::errors::is_transient;
use crate::nft_token_uri::resolve_ipfs_to_https;

/// Generous safety bounds on the `token_uri` JSON fields. The CH
/// `nft_enrichment.{name,collection_name,media_url}` columns are
/// `Nullable(String)` (unbounded), so these only sentinel pathological multi-KB
/// blobs — a long-but-valid metadata value is stored, not dropped.
const MAX_NAME_CHARS: usize = 4096;
const MAX_COLLECTION_CHARS: usize = 4096;
const MAX_MEDIA_URL_BYTES: usize = 8192;

/// Contract StrKey looked up by the NFT's `contract_id` FK on CH.
/// `nullIf(_, '')` collapses an empty/missing value to `None`.
#[derive(Row, Deserialize)]
struct StrkeyLookup {
    contract_strkey: Option<String>,
}

// The `#[instrument]` span carries the FULL composite key — every event in this
// fn inherits it, so individual events don't repeat the key.
#[instrument(skip(client, fetcher), fields(contract_id = key.contract_id, token_id = %key.token_id))]
pub async fn enrich_nft_token_uri(
    client: &Client,
    key: NftKey,
    fetcher: &NftTokenUriFetcher,
) -> Result<EnrichOutcome, EnrichError> {
    // The fetcher needs the contract StrKey to call `token_uri(token_id)`;
    // `nfts.contract_id` is the `soroban_contracts.id` FK.
    let lookup = client
        .query(
            "SELECT nullIf(contract_id, '') AS contract_strkey \
             FROM soroban_contracts FINAL WHERE id = ? LIMIT 1",
        )
        .bind(key.contract_id)
        .fetch_optional::<StrkeyLookup>()
        .await?;

    let Some(contract_strkey) = lookup.and_then(|l| l.contract_strkey) else {
        warn!(key = %key, reason = "contract_strkey_not_found", "writing sentinel");
        let (name, media_url, collection_name) = permanent_fail_outcome();
        return insert_nft(client, &key, name, media_url, collection_name).await;
    };

    let (name, media_url, mut collection_name) = match fetcher
        .resolve(&contract_strkey, &key.token_id)
        .await
    {
        Ok(Some(json)) => extract_columns(&json),
        // Fetcher honoured the convention but produced no JSON (reserved
        // for future variants). Permanent — sentinel write (was silent).
        Ok(None) => {
            debug!(key = %key, reason = "token_uri_no_json", "writing sentinel");
            permanent_fail_outcome()
        }
        // Transient (Http 5xx / connect / timeout, retryable SorobanRpc) →
        // bounce to SQS retry → DLQ → DepthAlarm.
        Err(arc_err) if is_transient(&arc_err) => {
            warn!(key = %key, reason = "transient", error = %arc_err, "retry candidate (no row written)");
            return Err(EnrichError::Transient(arc_err.to_string()));
        }
        // Permanent (4xx, malformed JSON, unsafe scheme, malformed
        // input, XDR codec, missing/contract-level RPC error) → sentinel.
        Err(arc_err) => {
            warn!(key = %key, reason = "token_uri_permanent", error = %arc_err, "sentinel written");
            permanent_fail_outcome()
        }
    };

    // The NFT collection name is written by the indexer, which runs the
    // contract's own `name()` locally into `soroban_contract_metadata` (task
    // 0620), and served via COALESCE over this column (Fix B / #331). The
    // `name()` RPC here is a FALLBACK for contracts the indexer does not answer
    // — a program declaring `name()` without `symbol()`, or a `name()` that
    // reads persistent data. Its write lands in `nft_enrichment.collection_name`,
    // which the read path COALESCEs UNDER the ledger name, so for a ledger-
    // covered contract this write is simply ignored at read time (harmless;
    // cached one RPC per contract; the worker runs at conc=0 regardless). No
    // real-world NFT carries `collection` in its token_uri JSON (0/68), so
    // `collection_name` is ~always empty here; a non-empty JSON value still
    // wins. Runs after the match on purpose: a permanent token_uri fail can
    // still yield a `name()` collection name.
    if collection_name.is_empty() {
        match fetcher.resolve_collection_name(&contract_strkey).await {
            Ok(Some(n)) => collection_name = n,
            // Permanent "no usable name" — cached in the fetcher; keep sentinel.
            Ok(None) => {
                debug!(key = %key, reason = "name_no_value", "collection_name stays sentinel")
            }
            // `resolve_collection_name` folds permanent fails to Ok(None), so
            // every Err is transient by contract — no re-classification here.
            Err(arc_err) => {
                warn!(key = %key, reason = "name_transient", error = %arc_err, "retry candidate (no row written)");
                return Err(EnrichError::Transient(arc_err.to_string()));
            }
        }
    }

    let outcome = insert_nft(client, &key, name, media_url, collection_name).await?;
    debug!("nft_enrichment row written");
    Ok(outcome)
}

/// The all-`''` outcome: a permanent fetch fail / missing contract / no JSON.
/// `''` per column is the "tried, nothing" sentinel (read-neutralised with
/// `NULLIF`). Mirrors `sep1_assets::permanent_fail_outcome`.
fn permanent_fail_outcome() -> (String, String, String) {
    (String::new(), String::new(), String::new())
}

/// Pull `name`, `image`, `collection` from the JSON blob; cap each at
/// the column width so an oversize value cannot break the INSERT.
///
/// `image` handling:
/// 1. `ipfs://...` values inside the metadata JSON are resolved to the
///    configured HTTPS gateway URL via [`resolve_ipfs_to_https`]. The
///    fetcher only resolves the *outer* `token_uri()` URI, so the
///    inner `image` field arrives unchanged here. Common NFT-metadata
///    convention (OpenSea / OpenZeppelin) stores `image` as
///    `ipfs://Qm.../1.png`, so without this step `media_url` would be
///    the empty sentinel for most real-world collections.
/// 2. The resolved value is then re-checked through the shared
///    [`super::is_safe_https_url`]: the frontend renders it as `<img src>`, so
///    anything other than `https://` (e.g. `http://`, `data:`, `javascript:`)
///    is replaced with the empty-string sentinel to avoid mixed-content
///    warnings and XSS vectors.
///
/// Returns `(name, image, collection)`; `""` is the "tried, nothing" sentinel.
fn extract_columns(json: &Value) -> (String, String, String) {
    let name = trimmed_string_chars(json.get("name"), MAX_NAME_CHARS);
    // `image` is the standard NFT-metadata media field. Fall back to `url` for
    // contracts that carry the image there instead (e.g. the CDA5FGE4 prototype,
    // whose token_uri JSON has the image CID under `url`, not `image` — the same
    // CID its separate `token_image()` entrypoint returns, so no extra RPC call
    // is needed). The fallback fires when `image` is absent OR present-but-empty
    // (an empty/whitespace/non-string `image` is as useless as a missing one).
    // `url` is non-standard / ambiguous (could be a website), but the
    // `resolve_ipfs_to_https` + `is_safe_https_url` guards below still apply, and
    // a wrong media_url is read-neutralised, not a correctness/security risk.
    let mut image_raw = trimmed_string_bytes(json.get("image"), MAX_MEDIA_URL_BYTES);
    if image_raw.is_empty() {
        image_raw = trimmed_string_bytes(json.get("url"), MAX_MEDIA_URL_BYTES);
    }
    let image_resolved = resolve_ipfs_to_https(&image_raw);
    let image = if image_resolved.is_empty() || super::is_safe_https_url(&image_resolved) {
        image_resolved
    } else {
        warn!(image = %image_raw, "unsafe media_url scheme; sentinel written");
        String::new()
    };
    let collection = trimmed_string_chars(json.get("collection"), MAX_COLLECTION_CHARS);
    (name, image, collection)
}

/// Caps by character count (not byte length) — a generous safety bound only
/// (the CH `nft_enrichment.{name,collection_name}` columns are unbounded
/// `Nullable(String)`; this just keeps a pathological multi-KB value out of the
/// row, it does not enforce a schema width). `chars().count()` so a long
/// multi-byte string is measured in characters, consistently.
fn trimmed_string_chars(v: Option<&Value>, max_chars: usize) -> String {
    let Some(s) = v.and_then(Value::as_str) else {
        return String::new();
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let count = trimmed.chars().count();
    if count > max_chars {
        warn!(
            chars = count,
            max = max_chars,
            "value exceeds the char cap; sentinel written"
        );
        return String::new();
    }
    trimmed.to_owned()
}

/// Byte-count cap for TEXT columns where the limit is a body-size
/// safeguard rather than a schema constraint.
fn trimmed_string_bytes(v: Option<&Value>, max_bytes: usize) -> String {
    let Some(s) = v.and_then(Value::as_str) else {
        return String::new();
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() > max_bytes {
        warn!(
            bytes = trimmed.len(),
            max = max_bytes,
            "value too long; sentinel written"
        );
        return String::new();
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests;
