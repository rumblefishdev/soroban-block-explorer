//! `nfts.{name, media_url, collection_name}` enrichment from per-token
//! `token_uri()` JSON metadata.
//!
//! Per task 0195 §2d.
//!
//! ### Hard-fail (NFT-only divergence from SEP-1)
//!
//! Every fetch / parse / validation failure propagates as
//! `EnrichError::Transient` → SQS retry → DLQ. **No sentinel write
//! path.** A row that fails to enrich stays NULL until manual DLQ
//! replay or 0196 backfill. The operator sees the failure via the
//! DepthAlarm; we never silently substitute `''` for missing data.
//!
//! Producer is an insert-hook on `nfts` mint events, so a row is
//! emitted exactly once per nft_id under normal operation —
//! permanent failures land 1 message in the DLQ, not a flood.
//!
//! ### Two `token_uri` response conventions
//!
//! Handled by the underlying [`NftTokenUriFetcher`]:
//!
//! - `application/json` — standard NFT metadata. Parse → extract
//!   `name`, `image` → `media_url`, `collection`. Unsafe `image`
//!   schemes (`http://`, `data:`, `javascript:`) → hard-fail.
//! - `image/*` — direct-image convention (e.g. JamesBachini Soroban
//!   example). The URI itself is the image; `name` and
//!   `collection_name` are absent — fetcher synthesises a JSON
//!   `{ "image": "<url>" }` so the worker writes only `media_url` and
//!   leaves the other two as empty (legitimate "field missing in
//!   source", NOT a sentinel).
//!
//! ### STUB STATUS
//!
//! `NftTokenUriFetcher::resolve()` currently returns
//! `Err(NotImplemented)` for every input until the Soroban RPC client
//! lands in this workspace. Beta callers see DLQ alarms / 502s —
//! intentional, not a regression.

use serde_json::Value;
use sqlx::{PgPool, Row};
use tracing::{debug, instrument, warn};

use super::EnrichError;
use crate::nft_token_uri::NftTokenUriFetcher;

/// `nfts.name VARCHAR(256)`.
const MAX_NAME_BYTES: usize = 256;
/// `nfts.collection_name VARCHAR(256)`.
const MAX_COLLECTION_BYTES: usize = 256;

#[instrument(skip(pool, fetcher), fields(nft_id))]
pub async fn enrich_nft_token_uri(
    pool: &PgPool,
    nft_id: i32,
    fetcher: &NftTokenUriFetcher,
) -> Result<(), EnrichError> {
    // Minimal up-front lookup: the fetcher needs `(contract_id, token_id)`
    // to call `token_uri(token_id)`. Re-using `nfts.contract_id → soroban_contracts(id)`
    // keeps the producer-emitted message body to a single integer.
    let row = sqlx::query(
        r#"
        SELECT sc.contract_id AS contract_strkey,
               n.token_id
          FROM nfts n
          JOIN soroban_contracts sc ON sc.id = n.contract_id
         WHERE n.id = $1
        "#,
    )
    .bind(nft_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        warn!("nft_id {nft_id} not found; acking SQS message");
        return Ok(());
    };

    let contract_strkey: String = row.try_get("contract_strkey")?;
    let token_id: String = row.try_get("token_id")?;

    let parsed = fetcher.resolve(&contract_strkey, &token_id).await;
    let (name, media_url, collection_name) = match parsed {
        Some(json) => extract_columns(&json),
        // Fetcher returned None — either a permanent fail (4xx, malformed
        // JSON, unsupported Content-Type) or the stub. Write all-empty
        // sentinels so the row records "tried, nothing available".
        None => (String::new(), String::new(), String::new()),
    };

    write_columns(pool, nft_id, &name, &media_url, &collection_name).await?;
    debug!(nft_id, "nft token_uri UPDATE applied");
    Ok(())
}

/// Pull `name`, `image`, `collection` from the JSON blob; cap each at
/// the column width so an oversize value cannot break the UPDATE.
///
/// `image` is additionally re-checked through [`is_safe_media_url`]:
/// the frontend renders it as `<img src>`, so anything other than
/// `https://` (e.g. `http://`, `data:`, `javascript:`) is replaced
/// with the empty-string sentinel to avoid mixed-content warnings
/// and XSS vectors. The fetcher already validates the outer
/// `token_uri()` URI and is expected to resolve any `ipfs://` form to
/// HTTPS before exposing it here, so this is defence in depth — same
/// pattern as `sep1_assets::is_safe_icon_url`.
fn extract_columns(json: &Value) -> (String, String, String) {
    let name = trimmed_string(json.get("name"), MAX_NAME_BYTES);
    let image_raw = trimmed_string(json.get("image"), 1024); // TEXT but cap to keep bodies reasonable
    let image = if image_raw.is_empty() || is_safe_media_url(&image_raw) {
        image_raw
    } else {
        warn!(image = %image_raw, "unsafe media_url scheme; sentinel written");
        String::new()
    };
    let collection = trimmed_string(json.get("collection"), MAX_COLLECTION_BYTES);
    (name, image, collection)
}

/// Frontend renders `media_url` as `<img src>`. Only `https://` passes —
/// `http://` is mixed-content; `javascript:` / `data:` are XSS vectors.
fn is_safe_media_url(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
}

fn trimmed_string(v: Option<&Value>, max_bytes: usize) -> String {
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

/// Single UPDATE writing all three columns with `real > sentinel > NULL`
/// priority via `COALESCE(NULLIF($n, ''), col, $n)` — same pattern as the
/// SEP-1 icon kind in `sep1_assets.rs`. The pattern matters even for the
/// nominally exactly-once insert-hook model:
///
/// - `NULLIF($n, '')` collapses an empty-string sentinel to `NULL`.
/// - `COALESCE(real, col, sentinel)` then prefers a real fetch result,
///   falls back to the existing column value, and only writes the
///   sentinel when nothing is there.
///
/// Net effect:
/// - First call lands real values → persisted. Re-delivery (SQS
///   visibility-timeout race, DLQ replay, 0196 backfill) cannot clobber
///   them with sentinels on a flap.
/// - First call lands sentinels → persisted. A later real fetch
///   (backfill) upgrades them. Sentinels are upgradable; real values stick.
async fn write_columns(
    pool: &PgPool,
    nft_id: i32,
    name: &str,
    media_url: &str,
    collection_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE nfts \
         SET name            = COALESCE(NULLIF($1, ''), name,            $1), \
             media_url       = COALESCE(NULLIF($2, ''), media_url,       $2), \
             collection_name = COALESCE(NULLIF($3, ''), collection_name, $3) \
         WHERE id = $4",
    )
    .bind(name)
    .bind(media_url)
    .bind(collection_name)
    .bind(nft_id)
    .execute(pool)
    .await?;
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
    fn trimmed_string_caps_oversize_to_sentinel() {
        let too_long = "x".repeat(MAX_NAME_BYTES + 1);
        let v = Value::String(too_long);
        assert_eq!(trimmed_string(Some(&v), MAX_NAME_BYTES), "");
    }

    #[test]
    fn trimmed_string_handles_non_string() {
        assert_eq!(trimmed_string(Some(&json!(42)), 256), "");
        assert_eq!(trimmed_string(Some(&json!(null)), 256), "");
        assert_eq!(trimmed_string(None, 256), "");
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
