//! `asset_enrichment` side-table fill from issuer SEP-1 stellar.toml.
//!
//! A single fetch yields both `image` and `name` from the matching
//! `CURRENCIES[]` row; both land in one `asset_enrichment` row (ADR 0050).
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

use super::persist::insert_asset;
use clickhouse::{Client, Row};
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use super::{AssetKey, EnrichError, EnrichOutcome};
use crate::http_transient::is_transient_reqwest;
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
) -> Result<EnrichOutcome, EnrichError> {
    // Issuer home_domain + StrKey drive the SEP-1 fetch + currency match.
    //
    // NO `FINAL` (task 0397): `id` is not the `accounts` sort key (that is
    // `account_id`), so `FINAL WHERE id = ?` has no key range to bound and
    // read-merges the whole table for ONE id — measured on prod 24.9M rows per
    // call, 100 BILLION over 7 days. The `idx_acc_id` bloom index turns the same
    // predicate into a seek: 24.6k rows, 15 ms, identical result. Bound the key,
    // THEN dedup — the house shape, see `api::common::ch` and
    // `assets::hydrate_sql` (task 0364).
    //
    // `ORDER BY last_seen_ledger DESC LIMIT 1` is the dedup: it picks the
    // RMT-latest version, needed because `home_domain` IS mutable (SET_OPTIONS —
    // 4 of 1.01M prod accounts carry more than one value). A bare `LIMIT 1 BY id`
    // would be wrong here for exactly that reason.
    let issuer = client
        .query(
            "SELECT nullIf(account_id, '')  AS issuer_strkey, \
                    nullIf(home_domain, '') AS home_domain \
             FROM accounts WHERE id = ? \
             ORDER BY last_seen_ledger DESC LIMIT 1",
        )
        .bind(key.issuer_id)
        .fetch_optional::<IssuerLookup>()
        .await?;

    let Some(issuer) = issuer else {
        warn!(key = %key, reason = "issuer_account_missing", "writing sentinel");
        let (icon, name) = permanent_fail_outcome(key.asset_type);
        return insert_asset(client, &key, icon, name).await;
    };

    let Some(home_domain) = issuer
        .home_domain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        // Was a silent sentinel write — now logged so a blank asset is
        // debuggable by key (in the `#[instrument]` span) + reason.
        debug!(key = %key, reason = "issuer_home_domain_missing", "writing sentinel");
        let (icon, name) = permanent_fail_outcome(key.asset_type);
        return insert_asset(client, &key, icon, name).await;
    };

    match fetcher.fetch(home_domain).await {
        Ok(parsed) => {
            let (icon, name) = resolve_currency_outcome(
                key.asset_type,
                Some(key.asset_code.as_str()),
                issuer.issuer_strkey.as_deref(),
                &parsed,
            );
            let outcome = insert_asset(client, &key, icon, name).await?;
            debug!("asset_enrichment row written");
            Ok(outcome)
        }
        Err(arc_err) => {
            if is_transient(&arc_err) {
                // Logged at the call site (was only sampled in the report) so
                // the retried key + cause are visible, not just counted.
                warn!(key = %key, reason = "transient", error = %arc_err, "retry candidate (no row written)");
                Err(EnrichError::Transient(arc_err.to_string()))
            } else {
                warn!(key = %key, reason = "sep1_fetch_permanent", error = %arc_err, "sentinel written");
                let (icon, name) = permanent_fail_outcome(key.asset_type);
                insert_asset(client, &key, icon, name).await
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
    if !super::is_safe_https_url(url) {
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

/// Fetch-level failures retry; parse-level failures are permanent — caller
/// writes the empty sentinel. Transport-error judgement is delegated to
/// [`crate::http_transient::is_transient_reqwest`] — the single rule shared
/// with the NFT-metadata path (5xx and 429 transient, other statuses
/// permanent; connect-level failures — the dead-issuer-domain signature —
/// permanent since 2026-08-11, see `http_transient.rs`).
/// `pub` so the ClickHouse paths classify fetch errors with the same rule as
/// the PG worker.
pub fn is_transient(err: &Sep1Error) -> bool {
    match err {
        Sep1Error::Timeout { .. } => true,
        Sep1Error::Http { source, .. } => is_transient_reqwest(source),
        Sep1Error::MissingHomeDomain
        | Sep1Error::MalformedHomeDomain { .. }
        | Sep1Error::BodyTooLarge { .. }
        | Sep1Error::NonUtf8Body
        // A refused redirect (off registrable-domain / unsafe target / over
        // budget, task 0200) is permanent — re-fetching yields the same 3xx.
        | Sep1Error::RedirectBlocked { .. }
        | Sep1Error::MalformedToml { .. } => false,
    }
}

#[cfg(test)]
mod tests;
