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
pub mod filter;
pub mod key;
pub mod message;
pub mod nft_collection_name;
pub mod nft_token_uri;
/// The single enrichment write surface (`asset_enrichment` / `nft_enrichment`
/// only). All `enrich_*` paths persist through here — one auditable blast-radius.
pub(crate) mod persist;
pub mod sep1_assets;

pub use error::EnrichError;
pub use key::{AssetKey, NftKey};
pub use message::EnrichmentMessage;

/// Terminal outcome of a single successful `enrich_*` call. Both variants
/// INSERT a side-table row (existence = "tried"); the split is what the
/// backfill **report** and the `status` query distinguish — they must not be
/// lumped (the report used to fold both into one "succeeded" count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichOutcome {
    /// At least one real value landed.
    Real,
    /// Only the `''` "tried, nothing" sentinel was written (permanent fail /
    /// no match). Read-neutralised with `NULLIF`.
    Sentinel,
}

use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds — the `version` for the enrichment side tables'
/// `ReplacingMergeTree(version)` (latest-write-wins). Enrichment has no ledger
/// context (it is an off-chain fetch, not triggered by a ledger), so wall-clock
/// time is its update clock — the enrichment analog of the `last_updated_ledger`
/// version used by the on-chain state tables.
///
/// CAVEAT — this is monotonic only *per machine*, NOT globally. Two writers can
/// touch the same key (the live worker Lambda and the operator-run
/// `backfill-enrichment-runner`). If their clocks are skewed, a writer with a
/// lagging clock can land a row with a LOWER `version` than an existing one and
/// lose the merge — e.g. a backfill that re-derives a real value could be
/// overwritten by (or fail to overwrite) an older sentinel. The values are
/// low-stakes (a logo / a name) and the skew window is small, but it is a real
/// gap: a `--force-retry`/drain MUST NOT run concurrently with the live worker
/// over overlapping keys (see 0301 ordering note). A future hardening is to
/// floor the version at `max(existing_version + 1, now_ms)` via a read-back.
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
