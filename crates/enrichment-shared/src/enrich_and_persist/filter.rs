//! Producer-side enrichment dedup filter (task 0231, step 6).
//!
//! The indexer producer extracts a batch's candidate keys and asks: which of
//! these have **no** `*_enrichment` row yet (= never tried)? Only those are
//! published to SQS, so the worker never re-fetches third-party data for an
//! entity that is already enriched — or already permanently failed (its `''`
//! sentinel row exists, so it is "tried" and correctly skipped).
//!
//! ## Mechanism (lore 0231 — 4/4 design-panel consensus)
//!
//! One per-batch anti-join `(key) NOT IN (SELECT key FROM *_enrichment)`. The
//! batch keys are streamed in as **four parallel typed arrays** zipped back
//! into tuples CH-side via `arrayZip`. This deliberately avoids both:
//!
//! - a client-bound `Array(Tuple)` — the `clickhouse` 0.15 crate has a
//!   None-in-tuple bind defect (the backfill runner sidesteps it too), and
//! - inline SQL literals — string-building `asset_code` would be an injection
//!   surface. The serializer quotes/escapes the `Array(String)` bind, so the
//!   key strings are injection-safe.
//!
//! ## Correctness
//!
//! The filter is an **optimisation, not a correctness gate**. The worker INSERT
//! is idempotent (`ReplacingMergeTree(version)`, latest-wins), so every race
//! (concurrent batch, SQS at-least-once redelivery, RMT merge lag) degrades to
//! a redundant fetch — never a missing or wrong row. Row existence is monotonic
//! (a row, sentinel included, is only ever added, never removed), so `FINAL` is
//! unnecessary for the existence test. The producer runs this **fail-open**: on
//! a CH error it publishes the full candidate set rather than drop enrichment.

use std::collections::HashSet;

use clickhouse::Client;

use super::{AssetKey, EnrichError, NftKey};

/// Of `candidates`, return the subset with no `asset_enrichment` row yet
/// (deduped). Empty input → empty output, no query issued.
///
/// One ClickHouse read per call. The caller owns the failure policy; the
/// producer treats an `Err` as fail-open (publish all candidates — over-fetch
/// is absorbed by the idempotent worker, dropping enrichment is not).
pub async fn unenriched_asset_keys(
    client: &Client,
    candidates: &[AssetKey],
) -> Result<Vec<AssetKey>, EnrichError> {
    // Dedup in-process first: a ledger batch re-lists the same asset many
    // times; collapsing to distinct keys is free and shrinks the bind payload.
    let mut seen: HashSet<&AssetKey> = HashSet::with_capacity(candidates.len());
    let unique: Vec<&AssetKey> = candidates.iter().filter(|k| seen.insert(k)).collect();
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    // Four parallel typed arrays, zipped back to tuples CH-side (see module doc).
    let asset_types: Vec<i16> = unique.iter().map(|k| k.asset_type).collect();
    let asset_codes: Vec<String> = unique.iter().map(|k| k.asset_code.clone()).collect();
    let issuer_ids: Vec<i64> = unique.iter().map(|k| k.issuer_id).collect();
    let contract_ids: Vec<i64> = unique.iter().map(|k| k.contract_id).collect();

    client
        .query(ASSET_UNENRICHED_QUERY)
        .bind(asset_types)
        .bind(asset_codes)
        .bind(issuer_ids)
        .bind(contract_ids)
        .fetch_all::<AssetKey>()
        .await
        .map_err(EnrichError::Database)
}

/// Candidate tuples (zipped from four bound arrays) minus the keys already in
/// `asset_enrichment`. The outer `SELECT` aliases match `AssetKey`'s field
/// names — `clickhouse::Row` validates column names, not just position.
const ASSET_UNENRICHED_QUERY: &str = "\
    SELECT t.1 AS asset_type, t.2 AS asset_code, t.3 AS issuer_id, t.4 AS contract_id \
    FROM ( \
        SELECT arrayJoin(arrayZip( \
            CAST(? AS Array(Int16)), \
            CAST(? AS Array(String)), \
            CAST(? AS Array(Int64)), \
            CAST(? AS Array(Int64)) \
        )) AS t \
    ) \
    WHERE (t.1, t.2, t.3, t.4) NOT IN ( \
        SELECT asset_type, asset_code, issuer_id, contract_id FROM asset_enrichment \
    )";

/// Of `candidates`, return the subset with no `nft_enrichment` row yet
/// (deduped). Empty input → empty output, no query issued. NFT counterpart of
/// [`unenriched_asset_keys`] — same mechanism, two-column `(contract_id,
/// token_id)` key.
pub async fn unenriched_nft_keys(
    client: &Client,
    candidates: &[NftKey],
) -> Result<Vec<NftKey>, EnrichError> {
    let mut seen: HashSet<&NftKey> = HashSet::with_capacity(candidates.len());
    let unique: Vec<&NftKey> = candidates.iter().filter(|k| seen.insert(k)).collect();
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    let contract_ids: Vec<i64> = unique.iter().map(|k| k.contract_id).collect();
    let token_ids: Vec<String> = unique.iter().map(|k| k.token_id.clone()).collect();

    client
        .query(NFT_UNENRICHED_QUERY)
        .bind(contract_ids)
        .bind(token_ids)
        .fetch_all::<NftKey>()
        .await
        .map_err(EnrichError::Database)
}

/// `nft_enrichment` anti-join — see [`ASSET_UNENRICHED_QUERY`]. Outer `SELECT`
/// aliases match `NftKey`'s field names for the named-column decode.
const NFT_UNENRICHED_QUERY: &str = "\
    SELECT t.1 AS contract_id, t.2 AS token_id \
    FROM ( \
        SELECT arrayJoin(arrayZip( \
            CAST(? AS Array(Int64)), \
            CAST(? AS Array(String)) \
        )) AS t \
    ) \
    WHERE (t.1, t.2) NOT IN ( \
        SELECT contract_id, token_id FROM nft_enrichment \
    )";

#[cfg(test)]
mod tests {
    use super::*;

    /// CH-backed smoke (task 0231 step 6): seed one enriched key, then confirm
    /// the anti-join keeps the new key and drops the enriched one. `#[ignore]`
    /// (needs a live local ClickHouse). Run:
    /// `CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
    ///  CLICKHOUSE_PASSWORD=clickhouse \
    ///  cargo test -p enrichment-shared filter_drops -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs a live local ClickHouse (asset_enrichment table)"]
    async fn filter_drops_enriched_keeps_new() {
        let cfg = db_clickhouse::Config::from_env();
        let client = db_clickhouse::client(&cfg);

        // Two distinct spike keys; seed only the first as "enriched".
        let enriched = AssetKey {
            asset_type: 1,
            asset_code: "__FLT_SEEDED".into(),
            issuer_id: 990_001,
            contract_id: 0,
        };
        let fresh = AssetKey {
            asset_type: 2,
            asset_code: "__FLT_FRESH".into(),
            issuer_id: 990_002,
            contract_id: 0,
        };

        client
            .query(
                "INSERT INTO asset_enrichment \
                 (asset_type, asset_code, issuer_id, contract_id, icon_url, name, version) \
                 VALUES (?, ?, ?, ?, NULL, NULL, now64(3))",
            )
            .bind(enriched.asset_type)
            .bind(&enriched.asset_code)
            .bind(enriched.issuer_id)
            .bind(enriched.contract_id)
            .execute()
            .await
            .expect("seed enriched key");

        // Pass duplicates too — the in-process dedup must collapse them.
        let candidates = vec![enriched.clone(), fresh.clone(), fresh.clone()];
        let missing = unenriched_asset_keys(&client, &candidates)
            .await
            .expect("anti-join query");

        assert_eq!(missing, vec![fresh.clone()], "only the fresh key survives");

        // Cleanup the spike row.
        client
            .query("ALTER TABLE asset_enrichment DELETE WHERE asset_code LIKE '__FLT_%'")
            .execute()
            .await
            .expect("cleanup");
    }

    /// NFT counterpart of `filter_drops_enriched_keeps_new`. `#[ignore]`.
    #[tokio::test]
    #[ignore = "needs a live local ClickHouse (nft_enrichment table)"]
    async fn nft_filter_drops_enriched_keeps_new() {
        let cfg = db_clickhouse::Config::from_env();
        let client = db_clickhouse::client(&cfg);

        let enriched = NftKey {
            contract_id: 990_101,
            token_id: "__FLT_SEEDED".into(),
        };
        let fresh = NftKey {
            contract_id: 990_102,
            token_id: "__FLT_FRESH".into(),
        };

        client
            .query(
                "INSERT INTO nft_enrichment \
                 (contract_id, token_id, name, media_url, collection_name, version) \
                 VALUES (?, ?, NULL, NULL, NULL, now64(3))",
            )
            .bind(enriched.contract_id)
            .bind(&enriched.token_id)
            .execute()
            .await
            .expect("seed enriched nft key");

        let candidates = vec![enriched.clone(), fresh.clone(), fresh.clone()];
        let missing = unenriched_nft_keys(&client, &candidates)
            .await
            .expect("nft anti-join query");

        assert_eq!(
            missing,
            vec![fresh.clone()],
            "only the fresh nft key survives"
        );

        client
            .query("ALTER TABLE nft_enrichment DELETE WHERE token_id LIKE '__FLT_%'")
            .execute()
            .await
            .expect("cleanup");
    }

    /// Empty input short-circuits before any query — so a never-connected
    /// default client is fine, and these run in CI (no live CH needed).
    #[tokio::test]
    async fn filter_empty_input_is_noop() {
        let client = clickhouse::Client::default();
        assert!(
            unenriched_asset_keys(&client, &[])
                .await
                .expect("empty assets")
                .is_empty()
        );
        assert!(
            unenriched_nft_keys(&client, &[])
                .await
                .expect("empty nfts")
                .is_empty()
        );
    }
}
