//! `nfts.{name, media_url, collection_name}` enrichment from per-token
//! `token_uri()` JSON metadata.
//!
//! Per task 0195 §2d.
//!
//! ### Failure model — soft-fail downstream of fetcher
//!
//! Producer is an insert-hook on `nfts` mint events, so a row is
//! emitted exactly once per nft_id under normal operation. Inside this
//! handler:
//!
//! - `fetcher.resolve()` returns `Result<Option<Value>, Arc<NftTokenUriError>>`.
//!   This handler dispatches via `is_transient`: transient errors
//!   (Http 5xx / connect / timeout, SorobanRpc) bubble as
//!   `EnrichError::Transient` so SQS retries; permanent errors (4xx,
//!   malformed JSON, unsafe scheme, malformed input, XDR codec) and
//!   `Ok(None)` write empty-string sentinels in
//!   `nfts.{name, media_url, collection_name}` so the row records
//!   "fetch attempted, no value" and the producer predicate
//!   `name IS NULL OR media_url IS NULL OR collection_name IS NULL`
//!   short-circuits on the next ledger touch.
//! - The `is_safe_media_url` re-check on the `image` field inside JSON
//!   metadata replaces unsafe schemes (`http://`, `data:`,
//!   `javascript:`) with the sentinel `''` — defence in depth against
//!   a contract smuggling a non-`https://` URL past the fetcher.
//! - `trimmed_string` caps each column at the schema width
//!   (VARCHAR(256) for `name` / `collection_name`, 1 KB for
//!   `media_url`) and writes the sentinel on overflow rather than
//!   letting the UPDATE fail.
//!
//! UPDATE uses `COALESCE(NULLIF($n, ''), col, $n)` per column —
//! priority `real > sentinel > NULL` (same shape as the §2a SEP-1
//! pipeline). A later DLQ replay or 0196 backfill that succeeds will
//! upgrade a sentinel-marked row in place; sentinels never clobber an
//! existing real value.
//!
//! ### Two `token_uri` response conventions
//!
//! Handled by the fetcher:
//!
//! - `application/json` — standard NFT metadata. Parse → extract
//!   `name`, `image` → `media_url`, `collection`.
//! - `image/*` — direct-image convention (e.g. JamesBachini Soroban
//!   example). The URI itself is the image; the fetcher synthesises a
//!   JSON `{ "image": "<url>" }` so this handler still writes only
//!   `media_url` and leaves `name` / `collection_name` as the
//!   "absent-in-source" sentinel.

use serde_json::Value;
use sqlx::{PgPool, Row};
use tracing::{debug, instrument, warn};

use super::EnrichError;
use crate::nft_token_uri::NftTokenUriFetcher;
use crate::nft_token_uri::errors::is_transient;
use crate::nft_token_uri::resolve_ipfs_to_https;

/// `nfts.name VARCHAR(256)` — Postgres VARCHAR limits character count, not bytes.
const MAX_NAME_CHARS: usize = 256;
/// `nfts.collection_name VARCHAR(256)`.
const MAX_COLLECTION_CHARS: usize = 256;
/// `nfts.media_url` is TEXT (no schema cap); the byte cap here just keeps
/// pathological URLs out of the row body.
const MAX_MEDIA_URL_BYTES: usize = 1024;

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

    let (name, media_url, collection_name) =
        match fetcher.resolve(&contract_strkey, &token_id).await {
            Ok(Some(json)) => extract_columns(&json),
            // Fetcher honoured the convention but produced no JSON (reserved
            // for future variants). Permanent — sentinel write so producer
            // dedup short-circuits.
            Ok(None) => (String::new(), String::new(), String::new()),
            // Transient (Http 5xx / connect / timeout, SorobanRpc) → bounce
            // to SQS retry → DLQ → DepthAlarm.
            Err(arc_err) if is_transient(&arc_err) => {
                return Err(EnrichError::Transient(arc_err.to_string()));
            }
            // Permanent (4xx, malformed JSON, unsafe scheme, malformed
            // input, XDR codec, etc.) → sentinel + warn. Operator can grep.
            Err(arc_err) => {
                warn!(error = %arc_err, "nft token_uri permanent fail; sentinel write");
                (String::new(), String::new(), String::new())
            }
        };

    write_columns(pool, nft_id, &name, &media_url, &collection_name).await?;
    debug!(nft_id, "nft token_uri UPDATE applied");
    Ok(())
}

/// Pull `name`, `image`, `collection` from the JSON blob; cap each at
/// the column width so an oversize value cannot break the UPDATE.
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
