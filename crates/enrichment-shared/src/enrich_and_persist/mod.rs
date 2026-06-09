//! Per-kind enrichment functions — the unit of work invoked by both the
//! type-1 worker Lambda (one call per SQS message) and any future local
//! backfill / refresh tool (one call per row pulled from a streaming
//! SELECT).
//!
//! Each `enrich_*` function owns the full "fetch externally + write the
//! target column(s)" path for a single row. Worker / backfill code only
//! has to drive iteration — they don't reimplement HTTP, parsing, or DB
//! writes.

pub mod error;
pub mod key;
pub mod message;
pub mod nft_token_uri;
pub mod sep1_assets;

pub use error::EnrichError;
pub use key::{AssetKey, NftKey};
pub use message::EnrichmentMessage;

use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds — the `version` for the enrichment side tables'
/// `ReplacingMergeTree(version)` (latest-write-wins). Enrichment has no ledger
/// context (it is an off-chain fetch, not triggered by a ledger), so wall-clock
/// time is its monotonic update clock — the enrichment analog of the
/// `last_updated_ledger` version used by the on-chain state tables.
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `true` iff `url` (trimmed, case-insensitive) uses the `https://` scheme.
/// The guard for user-facing URL columns the frontend renders as `<img src>`:
/// `http://` is mixed-content; `javascript:` / `data:` are XSS vectors. Shared
/// by `sep1_assets` (`icon_url`) and `nft_token_uri` (`media_url`).
pub(crate) fn is_safe_https_url(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::is_safe_https_url;

    #[test]
    fn is_safe_https_url_accepts_https() {
        assert!(is_safe_https_url("https://example.com/x.png"));
        assert!(is_safe_https_url("HTTPS://example.com/x.png"));
        assert!(is_safe_https_url("  https://gateway/ipfs/Qm.../x.png  "));
    }

    #[test]
    fn is_safe_https_url_rejects_unsafe_schemes() {
        assert!(!is_safe_https_url("http://example.com/x.png"));
        assert!(!is_safe_https_url("data:image/png;base64,iVBOR..."));
        assert!(!is_safe_https_url("javascript:alert(1)"));
        assert!(!is_safe_https_url("file:///etc/passwd"));
        assert!(!is_safe_https_url("ipfs://Qm.../x.png"));
        assert!(!is_safe_https_url(""));
    }
}
