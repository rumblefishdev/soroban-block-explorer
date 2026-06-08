//! `nft_enrichment` side-table fill from per-token `token_uri()` JSON
//! metadata (task 0195 §2d / ADR 0048).
//!
//! Writes `(name, media_url, collection_name)` into the `nft_enrichment`
//! side table — never the indexer-owned `nfts` table. These three are
//! **enrichment-only** (the indexer always writes `None`: a Stellar NFT
//! mint event carries no metadata), so the read path uses the side table
//! directly, no COALESCE to the indexer (task 0231).
//!
//! ### Failure model — soft-fail downstream of fetcher
//!
//! - transient errors (Http 5xx / connect / timeout, SorobanRpc) bubble as
//!   `EnrichError::Transient` so SQS retries;
//! - permanent errors (4xx, malformed JSON, unsafe scheme, XDR codec) and
//!   `Ok(None)` write the `''` sentinel, so the row records "fetch
//!   attempted, no value" and the candidate query (`NOT IN nft_enrichment`)
//!   skips the key on the next pass;
//! - `is_safe_media_url` replaces a non-`https://` `image` with the
//!   sentinel — defence in depth against a smuggled scheme.
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
use db_clickhouse::persist::enrichment::insert_nft_enrichment;
use db_clickhouse::persist::rows::NftEnrichmentRow;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, instrument, warn};

use super::{EnrichError, NftKey};
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
) -> Result<(), EnrichError> {
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
        warn!("soroban contract StrKey not found; writing sentinel");
        let (name, media_url, collection_name) = permanent_fail_outcome();
        return insert_columns(
            client,
            key.contract_id,
            &key.token_id,
            &name,
            &media_url,
            &collection_name,
        )
        .await;
    };

    let (name, media_url, collection_name) =
        match fetcher.resolve(&contract_strkey, &key.token_id).await {
            Ok(Some(json)) => extract_columns(&json),
            // Fetcher honoured the convention but produced no JSON (reserved
            // for future variants). Permanent — sentinel write.
            Ok(None) => permanent_fail_outcome(),
            // Transient (Http 5xx / connect / timeout, SorobanRpc) → bounce
            // to SQS retry → DLQ → DepthAlarm.
            Err(arc_err) if is_transient(&arc_err) => {
                return Err(EnrichError::Transient(arc_err.to_string()));
            }
            // Permanent (4xx, malformed JSON, unsafe scheme, malformed
            // input, XDR codec, etc.) → sentinel + warn. Operator can grep.
            Err(arc_err) => {
                warn!(error = %arc_err, "nft token_uri permanent fail; sentinel write");
                permanent_fail_outcome()
            }
        };

    insert_columns(
        client,
        key.contract_id,
        &key.token_id,
        &name,
        &media_url,
        &collection_name,
    )
    .await?;
    debug!("nft_enrichment row written");
    Ok(())
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
/// 2. The resolved value is then re-checked through [`is_safe_media_url`]:
///    the frontend renders it as `<img src>`, so anything other than
///    `https://` (e.g. `http://`, `data:`, `javascript:`) is replaced
///    with the empty-string sentinel to avoid mixed-content warnings
///    and XSS vectors. Same defence-in-depth pattern as
///    `sep1_assets::is_safe_icon_url`.
///
/// Returns `(name, image, collection)`; `""` is the "tried, nothing" sentinel.
fn extract_columns(json: &Value) -> (String, String, String) {
    let name = trimmed_string_chars(json.get("name"), MAX_NAME_CHARS);
    let image_raw = trimmed_string_bytes(json.get("image"), MAX_MEDIA_URL_BYTES);
    let image_resolved = resolve_ipfs_to_https(&image_raw);
    let image = if image_resolved.is_empty() || is_safe_media_url(&image_resolved) {
        image_resolved
    } else {
        warn!(image = %image_raw, "unsafe media_url scheme; sentinel written");
        String::new()
    };
    let collection = trimmed_string_chars(json.get("collection"), MAX_COLLECTION_CHARS);
    (name, image, collection)
}

/// Frontend renders `media_url` as `<img src>`. Only `https://` passes —
/// `http://` is mixed-content; `javascript:` / `data:` are XSS vectors.
fn is_safe_media_url(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
}

/// Postgres `VARCHAR(N)` caps character count, not byte length, so the
/// `name` and `collection_name` columns must measure with `chars().count()`.
/// Mismatched units would let a 256-char ASCII value pass and a 256-char
/// multi-byte value fail (or vice versa).
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
            "value too long for VARCHAR; sentinel written"
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

/// Build + INSERT one `nft_enrichment` row. `''` sentinels are stored as-is;
/// the read path neutralises them with `NULLIF`.
async fn insert_columns(
    client: &Client,
    contract_id: i64,
    token_id: &str,
    name: &str,
    media_url: &str,
    collection_name: &str,
) -> Result<(), EnrichError> {
    let row = NftEnrichmentRow {
        contract_id,
        token_id: token_id.to_owned(),
        name: Some(name.to_owned()),
        media_url: Some(media_url.to_owned()),
        collection_name: Some(collection_name.to_owned()),
        version: super::now_ms(),
    };
    insert_nft_enrichment(client, std::slice::from_ref(&row)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_columns_pulls_standard_fields() {
        let blob = json!({
            "name": "Punk #4521",
            "image": "https://example.com/4521.png",
            "collection": "CryptoPunks",
            "attributes": [{"trait_type": "Hat", "value": "Beanie"}]
        });
        let (name, image, collection) = extract_columns(&blob);
        assert_eq!(name, "Punk #4521");
        assert_eq!(image, "https://example.com/4521.png");
        assert_eq!(collection, "CryptoPunks");
    }

    #[test]
    fn extract_columns_returns_empty_for_missing_keys() {
        let blob = json!({});
        assert_eq!(
            extract_columns(&blob),
            (String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn extract_columns_trims_whitespace() {
        let blob = json!({"name": "  Spaced  ", "image": "", "collection": null});
        let (name, image, collection) = extract_columns(&blob);
        assert_eq!(name, "Spaced");
        assert_eq!(image, "");
        assert_eq!(collection, "");
    }

    #[test]
    fn trimmed_string_chars_caps_oversize_to_sentinel() {
        let too_long = "x".repeat(MAX_NAME_CHARS + 1);
        let v = Value::String(too_long);
        assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), "");
    }

    #[test]
    fn trimmed_string_chars_uses_char_count_not_byte_length() {
        // 256 multi-byte chars (each emoji = 4 bytes) → 1024 bytes but 256 chars.
        // Char-cap MUST accept this (matches VARCHAR(256) capacity); byte-cap
        // would have wrongly rejected it.
        let exactly_max = "🚀".repeat(MAX_NAME_CHARS);
        assert_eq!(exactly_max.chars().count(), MAX_NAME_CHARS);
        assert!(exactly_max.len() > MAX_NAME_CHARS); // confirm bytes > chars
        let v = Value::String(exactly_max.clone());
        assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), exactly_max);

        // One char over the cap → sentinel.
        let over = "🚀".repeat(MAX_NAME_CHARS + 1);
        let v = Value::String(over);
        assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), "");
    }

    #[test]
    fn trimmed_string_chars_handles_non_string() {
        assert_eq!(trimmed_string_chars(Some(&json!(42)), 256), "");
        assert_eq!(trimmed_string_chars(Some(&json!(null)), 256), "");
        assert_eq!(trimmed_string_chars(None, 256), "");
    }

    #[test]
    fn trimmed_string_bytes_caps_for_text_columns() {
        let too_long = "x".repeat(MAX_MEDIA_URL_BYTES + 1);
        let v = Value::String(too_long);
        assert_eq!(trimmed_string_bytes(Some(&v), MAX_MEDIA_URL_BYTES), "");
    }

    #[test]
    fn is_safe_media_url_accepts_https() {
        assert!(is_safe_media_url("https://example.com/x.png"));
        assert!(is_safe_media_url("HTTPS://example.com/x.png"));
        assert!(is_safe_media_url("  https://gateway/ipfs/Qm.../x.png  "));
    }

    #[test]
    fn is_safe_media_url_rejects_unsafe_schemes() {
        assert!(!is_safe_media_url("http://example.com/x.png"));
        assert!(!is_safe_media_url("data:image/png;base64,iVBOR..."));
        assert!(!is_safe_media_url("javascript:alert(1)"));
        assert!(!is_safe_media_url("file:///etc/passwd"));
        assert!(!is_safe_media_url("ipfs://Qm.../x.png")); // expected pre-resolved by fetcher
        assert!(!is_safe_media_url(""));
    }

    #[test]
    fn extract_columns_resolves_ipfs_image_to_https() {
        let blob = json!({
            "name": "Punk #1",
            "image": "ipfs://QmFoo/1.png",
            "collection": "X"
        });
        let (_, image, _) = extract_columns(&blob);
        assert!(
            image.starts_with("https://"),
            "ipfs:// must be resolved, got {image}"
        );
        assert!(image.ends_with("QmFoo/1.png"));
    }

    #[test]
    fn extract_columns_replaces_unsafe_image_with_sentinel() {
        let blob = json!({
            "name": "Punk",
            "image": "javascript:alert(1)",
            "collection": "X"
        });
        let (name, image, collection) = extract_columns(&blob);
        assert_eq!(name, "Punk");
        assert_eq!(image, ""); // sentinel — not the malicious scheme
        assert_eq!(collection, "X");
    }
}
