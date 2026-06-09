//! `asset_enrichment` side-table fill from issuer SEP-1 stellar.toml.
//!
//! A single fetch yields both `image` and `name` from the matching
//! `CURRENCIES[]` row; both land in one `asset_enrichment` row (ADR 0048).
//! The indexer-owned `assets` table is **never** written here, so the
//! indexer's continuous whole-row re-inserts cannot clobber the
//! enrichment.
//!
//! `name` is filled only for ClassicCredit (asset_type=1) and SAC
//! (asset_type=2). Native (0) / soroban (3) are not SEP-1 enrichable —
//! their names come from elsewhere (Option C: soroban from
//! `soroban_contracts.name`, native from an API constant; task 0231).
//! Permanent fails write the `''` sentinel; a row **existing** for the key
//! marks it "tried" so the candidate query (`NOT IN asset_enrichment`)
//! skips it. Transient fails return [`EnrichError::Transient`] and SQS
//! retries.
//!
//! The side table is `ReplacingMergeTree(version)`: every write is an
//! INSERT with `version = now_ms`, latest-wins. The read path neutralises
//! the `''` sentinel with `NULLIF` (task 0243). A later run upgrades a
//! sentinel to a real value — or clears a removed one — simply by
//! inserting a newer-version row.

use clickhouse::{Client, Row};
use db_clickhouse::persist::enrichment::insert_asset_enrichment;
use db_clickhouse::persist::rows::AssetEnrichmentRow;
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use super::{AssetKey, EnrichError};
use crate::sep1::dto::Sep1Currency;
use crate::sep1::errors::Sep1Error;
use crate::sep1::{Sep1Fetcher, Sep1TomlParsed};

/// Generous safety bound on the SEP-1 `image` URL. CH `asset_enrichment.icon_url`
/// is `Nullable(String)` (unbounded), so this only sentinels pathological
/// multi-KB blobs — a long-but-valid URL is stored, not dropped.
const MAX_ICON_URL_BYTES: usize = 8192;
/// Generous safety bound on the SEP-1 `name` (CH `asset_enrichment.name` is
/// unbounded `Nullable(String)`).
const MAX_NAME_BYTES: usize = 4096;

/// Issuer inputs for SEP-1 resolution, looked up by `issuer_id` on CH.
/// `nullIf(_, '')` collapses an empty/missing value to `None`.
#[derive(Row, Deserialize)]
struct IssuerLookup {
    issuer_strkey: Option<String>,
    home_domain: Option<String>,
}

// The `#[instrument]` span carries the FULL composite key — every event in this
// fn (incl. the resolver warnings) inherits it, so individual events don't repeat
// the key.
#[instrument(
    skip(client, fetcher),
    fields(
        asset_type = key.asset_type,
        asset_code = %key.asset_code,
        issuer_id = key.issuer_id,
        contract_id = key.contract_id,
    )
)]
pub async fn enrich_asset_from_sep1(
    client: &Client,
    key: AssetKey,
    fetcher: &Sep1Fetcher,
) -> Result<(), EnrichError> {
    // Issuer home_domain + StrKey drive the SEP-1 fetch + currency match.
    let issuer = client
        .query(
            "SELECT nullIf(account_id, '')  AS issuer_strkey, \
                    nullIf(home_domain, '') AS home_domain \
             FROM accounts FINAL WHERE id = ? LIMIT 1",
        )
        .bind(key.issuer_id)
        .fetch_optional::<IssuerLookup>()
        .await?;

    let Some(issuer) = issuer else {
        warn!("issuer account not found; writing sentinel");
        let (icon, name) = permanent_fail_outcome(key.asset_type);
        return insert_outcome(client, &key, icon, name).await;
    };

    let Some(home_domain) = issuer
        .home_domain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        let (icon, name) = permanent_fail_outcome(key.asset_type);
        return insert_outcome(client, &key, icon, name).await;
    };

    match fetcher.fetch(home_domain).await {
        Ok(parsed) => {
            let (icon, name) = resolve_currency_outcome(
                key.asset_type,
                Some(key.asset_code.as_str()),
                issuer.issuer_strkey.as_deref(),
                &parsed,
            );
            insert_outcome(client, &key, icon, name).await?;
            debug!("asset_enrichment row written");
            Ok(())
        }
        Err(arc_err) => {
            if is_transient(&arc_err) {
                Err(EnrichError::Transient(arc_err.to_string()))
            } else {
                warn!("permanent SEP-1 fetch failure: {arc_err}; sentinel written");
                let (icon, name) = permanent_fail_outcome(key.asset_type);
                insert_outcome(client, &key, icon, name).await
            }
        }
    }
}

/// Resolve the `(icon_url, name)` write outcome from a fetched SEP-1 TOML —
/// the security-sensitive icon/name validation (https-only, length caps,
/// sentinel rules) in one place. `""` = the "tried, nothing" sentinel;
/// `name = None` for native/soroban (not SEP-1 enrichable). See module doc.
fn resolve_currency_outcome(
    asset_type: i16,
    asset_code: Option<&str>,
    issuer_strkey: Option<&str>,
    parsed: &Sep1TomlParsed,
) -> (String, Option<String>) {
    let entry = find_currency(parsed, asset_code, issuer_strkey);
    (resolve_icon(entry), resolve_name(asset_type, entry))
}

/// The outcome to write when the issuer has no usable home_domain or the SEP-1
/// fetch permanently failed: the `""` icon sentinel + the per-type name
/// sentinel. Mirrors `nft_token_uri::permanent_fail_outcome`.
fn permanent_fail_outcome(asset_type: i16) -> (String, Option<String>) {
    (String::new(), sentinel_name(asset_type))
}

/// `Some("")` is the "tried, nothing" sentinel for classic/SAC — the row's
/// existence makes the candidate query (`NOT IN asset_enrichment`) skip the key
/// next pass. `None` for native/soroban (0/3): not SEP-1 enrichable; their names
/// come from elsewhere (Option C — `soroban_contracts.name` / an API constant).
fn sentinel_name(asset_type: i16) -> Option<String> {
    matches!(asset_type, 1 | 2).then(String::new)
}

fn resolve_icon(entry: Option<&Sep1Currency>) -> String {
    let Some(url) = entry
        .and_then(|c| c.image.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return String::new();
    };
    if !is_safe_icon_url(url) {
        warn!(
            url_prefix = url.chars().take(20).collect::<String>(),
            "icon URL not https://; sentinel written (potential XSS)",
        );
        return String::new();
    }
    if url.len() > MAX_ICON_URL_BYTES {
        warn!(
            bytes = url.len(),
            max = MAX_ICON_URL_BYTES,
            "icon URL too long; sentinel written"
        );
        return String::new();
    }
    url.to_owned()
}

fn resolve_name(asset_type: i16, entry: Option<&Sep1Currency>) -> Option<String> {
    if !matches!(asset_type, 1 | 2) {
        return None;
    }
    let trimmed = entry
        .and_then(|c| c.name.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(name) = trimmed else {
        return Some(String::new());
    };
    if name.len() > MAX_NAME_BYTES {
        warn!(
            bytes = name.len(),
            max = MAX_NAME_BYTES,
            "SEP-1 name too long; sentinel written"
        );
        return Some(String::new());
    }
    Some(name.to_owned())
}

/// Build + INSERT one `asset_enrichment` row. `icon`/`name` come straight
/// from the resolver — the `''` / `Some("")` sentinels are stored as-is; the
/// read path neutralises them with `NULLIF`.
async fn insert_outcome(
    client: &Client,
    key: &AssetKey,
    icon_url: String,
    name: Option<String>,
) -> Result<(), EnrichError> {
    let row = AssetEnrichmentRow {
        asset_type: key.asset_type,
        asset_code: key.asset_code.clone(),
        issuer_id: key.issuer_id,
        contract_id: key.contract_id,
        icon_url: Some(icon_url),
        name,
        version: super::now_ms(),
    };
    insert_asset_enrichment(client, std::slice::from_ref(&row)).await?;
    Ok(())
}

/// Frontend renders `icon_url` as `<img src>`. Only `https://` passes —
/// `http://` is mixed-content; `javascript:` / `data:` are XSS vectors.
fn is_safe_icon_url(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("https://")
}

fn find_currency<'a>(
    parsed: &'a Sep1TomlParsed,
    code: Option<&str>,
    issuer: Option<&str>,
) -> Option<&'a Sep1Currency> {
    let (code, issuer) = (code?, issuer?);
    parsed
        .currencies
        .iter()
        .find(|c| c.code.as_deref() == Some(code) && c.issuer.as_deref() == Some(issuer))
}

/// Network-layer (no HTTP status) and 5xx retry. 4xx and parse-level
/// failures are permanent — caller writes the empty sentinel. `pub` so the
/// ClickHouse paths classify fetch errors with the same rule as the PG worker.
pub fn is_transient(err: &Sep1Error) -> bool {
    match err {
        Sep1Error::Timeout { .. } => true,
        Sep1Error::Http { source, .. } => match source.status() {
            Some(s) if s.is_server_error() => true,
            Some(_) => false,
            None => true,
        },
        Sep1Error::MissingHomeDomain
        | Sep1Error::MalformedHomeDomain { .. }
        | Sep1Error::BodyTooLarge { .. }
        | Sep1Error::NonUtf8Body
        | Sep1Error::MalformedToml { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toml_with(
        code: &str,
        issuer: &str,
        image: Option<&str>,
        name: Option<&str>,
    ) -> Sep1TomlParsed {
        Sep1TomlParsed {
            currencies: vec![Sep1Currency {
                code: Some(code.to_owned()),
                issuer: Some(issuer.to_owned()),
                desc: None,
                image: image.map(str::to_owned),
                name: name.map(str::to_owned),
            }],
            documentation: None,
        }
    }

    fn entry<'a>(p: &'a Sep1TomlParsed, code: &str, issuer: &str) -> Option<&'a Sep1Currency> {
        find_currency(p, Some(code), Some(issuer))
    }

    #[test]
    fn find_currency_matches_by_code_and_issuer() {
        let p = toml_with("USDC", "GA1", Some("https://x/u.png"), Some("USD Coin"));
        assert!(find_currency(&p, Some("USDC"), Some("GA1")).is_some());
        assert!(find_currency(&p, Some("EURC"), Some("GA1")).is_none());
        assert!(find_currency(&p, Some("USDC"), Some("GA2")).is_none());
        assert!(find_currency(&p, None, None).is_none());
    }

    #[test]
    fn resolve_icon_returns_url_on_match() {
        let p = toml_with("USDC", "GA1", Some("https://example.com/u.png"), None);
        assert_eq!(
            resolve_icon(entry(&p, "USDC", "GA1")),
            "https://example.com/u.png"
        );
    }

    #[test]
    fn resolve_icon_returns_sentinel_when_missing_or_empty() {
        let none = toml_with("USDC", "GA1", None, None);
        let empty = toml_with("USDC", "GA1", Some(""), None);
        assert_eq!(resolve_icon(entry(&none, "USDC", "GA1")), "");
        assert_eq!(resolve_icon(entry(&empty, "USDC", "GA1")), "");
        assert_eq!(resolve_icon(None), "");
    }

    #[test]
    fn resolve_icon_rejects_unsafe_scheme() {
        let p = toml_with("USDC", "GA1", Some("javascript:alert(1)"), None);
        assert_eq!(resolve_icon(entry(&p, "USDC", "GA1")), "");
    }

    #[test]
    fn resolve_icon_rejects_too_long() {
        let url = format!("https://example.com/{}", "x".repeat(MAX_ICON_URL_BYTES));
        let p = toml_with("USDC", "GA1", Some(&url), None);
        assert_eq!(resolve_icon(entry(&p, "USDC", "GA1")), "");
    }

    #[test]
    fn resolve_name_skips_native_and_soroban() {
        let p = toml_with("USDC", "GA1", None, Some("USD Coin"));
        assert!(resolve_name(0, entry(&p, "USDC", "GA1")).is_none());
        assert!(resolve_name(3, entry(&p, "USDC", "GA1")).is_none());
    }

    #[test]
    fn resolve_name_fills_for_classic_and_sac() {
        let p = toml_with("USDC", "GA1", None, Some("USD Coin"));
        for at in [1, 2] {
            assert_eq!(
                resolve_name(at, entry(&p, "USDC", "GA1")).as_deref(),
                Some("USD Coin")
            );
        }
    }

    #[test]
    fn resolve_name_trims_whitespace() {
        let p = toml_with("USDC", "GA1", None, Some("  USD Coin  "));
        assert_eq!(
            resolve_name(1, entry(&p, "USDC", "GA1")).as_deref(),
            Some("USD Coin")
        );
    }

    #[test]
    fn resolve_name_sentinel_when_missing_or_blank() {
        let none = toml_with("USDC", "GA1", None, None);
        let empty = toml_with("USDC", "GA1", None, Some(""));
        let ws = toml_with("USDC", "GA1", None, Some("   "));
        for p in [&none, &empty, &ws] {
            assert_eq!(
                resolve_name(1, entry(p, "USDC", "GA1")).as_deref(),
                Some("")
            );
        }
        assert_eq!(resolve_name(1, None).as_deref(), Some(""));
    }

    #[test]
    fn resolve_name_sentinel_when_too_long() {
        let too_long = "x".repeat(MAX_NAME_BYTES + 1);
        let p = toml_with("USDC", "GA1", None, Some(&too_long));
        assert_eq!(
            resolve_name(1, entry(&p, "USDC", "GA1")).as_deref(),
            Some("")
        );
    }

    #[test]
    fn sentinel_name_only_for_classic_and_sac() {
        assert!(sentinel_name(0).is_none());
        assert_eq!(sentinel_name(1).as_deref(), Some(""));
        assert_eq!(sentinel_name(2).as_deref(), Some(""));
        assert!(sentinel_name(3).is_none());
    }
}
