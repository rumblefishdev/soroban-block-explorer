//! `nft_enrichment` side-table fill from per-token `token_uri()` JSON
//! metadata (task 0195 §2d / ADR 0050).
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

    let (name, media_url, collection_name) = match fetcher
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
    // is needed). `url` is non-standard / ambiguous (could be a website), but
    // the `resolve_ipfs_to_https` + `is_safe_https_url` guards below still apply,
    // and a wrong media_url is read-neutralised, not a correctness/security risk.
    let image_raw = trimmed_string_bytes(
        json.get("image").or_else(|| json.get("url")),
        MAX_MEDIA_URL_BYTES,
    );
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
        // `MAX_NAME_CHARS` multi-byte chars (each emoji = 4 bytes) → 4× bytes but
        // exactly the char cap. Char-cap MUST accept this; a byte-cap would have
        // wrongly rejected it.
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

    /// CH-backed end-to-end smoke (task 0231 step 5): the full nft write path
    /// against a live local ClickHouse — `soroban_contracts` StrKey lookup →
    /// live `token_uri()` RPC (+ metadata fetch) → INSERT → read back. Covers
    /// the **real** path (a known mainnet contract) and the **sentinel** path
    /// (contract absent from `soroban_contracts`). `#[ignore]` (needs CH +
    /// mainnet Soroban-RPC + reachable token_uri metadata). Run:
    /// `CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
    ///  CLICKHOUSE_PASSWORD=clickhouse cargo test -p enrichment-shared \
    ///  smoke_ch_nft -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs live local ClickHouse + mainnet Soroban-RPC + reachable token_uri metadata"]
    async fn smoke_ch_nft_real_and_sentinel() {
        #[derive(Row, Deserialize)]
        struct Readback {
            name: Option<String>,
            media_url: Option<String>,
            collection_name: Option<String>,
        }
        async fn read(
            client: &Client,
            k: &NftKey,
        ) -> (Option<String>, Option<String>, Option<String>) {
            let r = client
                .query(
                    "SELECT name, media_url, collection_name FROM nft_enrichment FINAL \
                     WHERE contract_id = ? AND token_id = ?",
                )
                .bind(k.contract_id)
                .bind(&k.token_id)
                .fetch_one::<Readback>()
                .await
                .expect("read nft_enrichment");
            (r.name, r.media_url, r.collection_name)
        }

        let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
        let fetcher = NftTokenUriFetcher::new().expect("build fetcher");

        // --- REAL: seed a known mainnet NFT contract (0-arg token_uri; the
        // fetcher's arity fallback handles it) ---
        let contract = "CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY";
        let contract_id = db_clickhouse::persist::ids::contract_id(contract);
        client
            .query(
                "INSERT INTO soroban_contracts (id, contract_id, wasm_uploaded_at_ledger, is_sac) \
                 VALUES (?, ?, 0, false)",
            )
            .bind(contract_id)
            .bind(contract)
            .execute()
            .await
            .expect("seed soroban contract");

        let key = NftKey {
            contract_id,
            token_id: "1".into(),
        };
        enrich_nft_token_uri(&client, key.clone(), &fetcher)
            .await
            .expect("enrich nft");
        let (name, media, coll) = read(&client, &key).await;
        eprintln!("REAL NFT -> name={name:?} media={media:?} coll={coll:?}");
        assert!(
            [&name, &media, &coll]
                .iter()
                .any(|c| c.as_deref().is_some_and(|s| !s.is_empty())),
            "real path: at least one non-empty metadata field \
             (depends on the contract's token_uri target staying reachable)"
        );

        // --- SENTINEL: a contract absent from `soroban_contracts` ---
        let ghost = NftKey {
            contract_id: 808_001,
            token_id: "1".into(),
        };
        enrich_nft_token_uri(&client, ghost.clone(), &fetcher)
            .await
            .expect("enrich ghost nft");
        let (gn, gm, gc) = read(&client, &ghost).await;
        assert_eq!(
            (gn.as_deref(), gm.as_deref(), gc.as_deref()),
            (Some(""), Some(""), Some("")),
            "sentinel: all columns = ''"
        );

        // --- cleanup (dev CH is otherwise empty) ---
        client
            .query("ALTER TABLE soroban_contracts DELETE WHERE id = ?")
            .bind(contract_id)
            .execute()
            .await
            .expect("cleanup soroban_contracts");
        client
            .query("ALTER TABLE nft_enrichment DELETE WHERE token_id = '1'")
            .execute()
            .await
            .expect("cleanup nft_enrichment");
    }
}
