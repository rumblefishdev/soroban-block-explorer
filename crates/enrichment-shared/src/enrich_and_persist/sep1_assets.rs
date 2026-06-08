//! `assets.{icon_url, name}` enrichment from issuer SEP-1 stellar.toml.
//!
//! Single fetch yields both `image` and `name` from the matching
//! `CURRENCIES[]` row; the worker writes them in one UPDATE so SQS
//! traffic stays at one message per asset.
//!
//! `assets.name` is filled only for ClassicCredit (asset_type=1) and
//! SAC (asset_type=2). Soroban-native (3) is owned by the indexer
//! (task 0156). Permanent fails write the `''` sentinel so the
//! producer dedup
//! `WHERE icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL)`
//! short-circuits. Transient fails return [`EnrichError::Transient`]
//! and SQS retries.
//!
//! UPDATE uses `COALESCE(NULLIF($n, ''), col, $n)` per column so the
//! write ordering is `real > sentinel > NULL`:
//!
//! - real beats existing sentinel `''` — a later run that finally
//!   resolves a column upgrades the row instead of being blocked by
//!   the previous permanent-fail sentinel.
//! - real beats existing real — a later TOML edit refreshes the value.
//! - sentinel does NOT clobber existing real — protects classic-form
//!   assets re-emitted because the OTHER column was NULL (e.g.
//!   `icon_url` already real, `name` missing → producer re-emits;
//!   worker permanent-fails on `image` lookup but real `icon_url`
//!   stays put).
//! - sentinel beats NULL — first non-success run records the
//!   producer-dedup breaker.
//!
//! For Soroban-native (asset_type=3) the binding for `name` is `NULL`,
//! so `COALESCE` is a no-op and indexer-set names (task 0156) are
//! untouched.

use sqlx::{PgPool, Row};
use tracing::{debug, instrument, warn};

use super::EnrichError;
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

#[instrument(skip(pool, fetcher), fields(asset_id))]
pub async fn enrich_asset_from_sep1(
    pool: &PgPool,
    asset_id: i32,
    fetcher: &Sep1Fetcher,
) -> Result<(), EnrichError> {
    let row = sqlx::query(
        r#"
        SELECT
            a.asset_type,
            a.asset_code,
            iss.account_id  AS issuer_strkey,
            iss.home_domain
        FROM assets a
        LEFT JOIN accounts iss ON iss.id = a.issuer_id
        WHERE a.id = $1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        warn!("asset_id {asset_id} not found; acking SQS message");
        return Ok(());
    };

    let asset_type: i16 = row.try_get("asset_type")?;
    let asset_code: Option<String> = row.try_get("asset_code")?;
    let issuer_strkey: Option<String> = row.try_get("issuer_strkey")?;
    let home_domain: Option<String> = row.try_get("home_domain")?;

    let Some(home_domain) = home_domain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        let (icon, name) = permanent_fail_outcome(asset_type);
        write_outcome(pool, asset_id, &icon, name).await?;
        return Ok(());
    };

    match fetcher.fetch(home_domain).await {
        Ok(parsed) => {
            let (icon, name) = resolve_currency_outcome(
                asset_type,
                asset_code.as_deref(),
                issuer_strkey.as_deref(),
                &parsed,
            );
            write_outcome(pool, asset_id, &icon, name).await?;
            debug!(asset_id, asset_type, "icon+name UPDATE applied");
            Ok(())
        }
        Err(arc_err) => {
            if is_transient(&arc_err) {
                Err(EnrichError::Transient(arc_err.to_string()))
            } else {
                warn!("permanent SEP-1 fetch failure: {arc_err}; sentinels written");
                let (icon, name) = permanent_fail_outcome(asset_type);
                write_outcome(pool, asset_id, &icon, name).await?;
                Ok(())
            }
        }
    }
}

/// Resolve the `(icon_url, name)` write outcome from a fetched SEP-1 TOML.
/// `pub` so the ClickHouse drain (`backfill-enrichment-runner`) reuses the
/// security-sensitive icon/name validation (https-only, length caps, sentinel
/// rules) rather than duplicating it. `""` = the `real > sentinel > NULL`
/// sentinel; `name = None` for native/soroban (indexer-owned). See module doc.
pub fn resolve_currency_outcome(
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
/// sentinel. Storage-agnostic (same as [`resolve_currency_outcome`]).
pub fn permanent_fail_outcome(asset_type: i16) -> (String, Option<String>) {
    (String::new(), sentinel_name(asset_type))
}

/// `Some("")` sentinel breaks the producer dedup loop on
/// `asset_type IN (1, 2) AND name IS NULL`; `None` is a no-op via
/// `COALESCE`, preserving indexer-set Soroban-native names.
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

/// Write priority `real > sentinel > NULL` per column — see module doc.
async fn write_outcome(
    pool: &PgPool,
    asset_id: i32,
    icon_url: &str,
    name: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE assets \
         SET icon_url = COALESCE(NULLIF($1, ''), icon_url, $1), \
             name     = COALESCE(NULLIF($2, ''), name,     $2) \
         WHERE id = $3",
    )
    .bind(icon_url)
    .bind(name)
    .bind(asset_id)
    .execute(pool)
    .await?;
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
