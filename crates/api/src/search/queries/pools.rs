//! Search's pool bucket: an exact pool id, or pools matched by asset code.

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use super::IncludeFlags;
use crate::common::asset_identity::{ResolvedAsset, leg_label, resolve_asset_identities};
use crate::common::pool_asset_codes::{normalize_asset_codes, pool_asset_filter};
use crate::common::strkey::{decode_pool_kind, pool_id_hex_to_strkey};
use crate::search::classifier::Classified;
use crate::search::dto::{EntityType, SearchHit};

// ---------------------------------------------------------------------------
// Pools — exact pool_id (from a 64-hex or full L-StrKey) → PK point seek
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct PoolRow {
    pool_hex: String,
    pool_kind: i16,
    legs: Vec<i64>,
}

/// The pool's display name is composed in RUST, from the same resolved leg
/// identities the pools list renders — not from a SQL expression over the pair
/// columns, which was a fourth copy of the "native renders as XLM" rule and
/// could only ever name a classic pool (a soroban row's pair columns are
/// placeholders that read as XLM/XLM).
///
/// # Two shapes, one entity
///
/// A hash-shaped query is a point seek on the primary key and stays exactly as
/// it was. Anything else is treated as an asset code and matched with the SAME
/// rule the pools list uses (`common::pool_asset_codes`, task 0470) — before
/// that, a non-hash query matched no pool at all, so `KALE` reported zero here
/// while the pools page listed 58, which read as the 0440 fix not working.
pub(super) async fn search_pools(
    client: &clickhouse::Client,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.pool {
        return Ok(Vec::new());
    }
    match classified.hash_bytes.as_deref() {
        Some(bytes) => search_pool_by_id(client, bytes).await,
        None => search_pools_by_asset_code(client, q, per_group_limit).await,
    }
}

/// Pools whose displayed asset codes match the query. Full scan of
/// `liquidity_pools` — 52 472 pools / 73 880 rows, measured at 47 ms and
/// 3.3 MiB on production, on the arm that previously did nothing for this
/// input.
///
/// `argMax(… , last_updated_ledger)` per `pool_id` collapses re-ingest
/// versions on read: the table carries multiple rows per pool (measured: 73 880
/// rows for 52 472 pools) and matching without the grouping would return the
/// same pool several times.
///
/// The pools LIST dedups the same table with `FINAL` instead. That is right
/// there — it reads one page through the primary key — and wrong here, where
/// the predicate cannot be pushed down and `FINAL` would merge the whole table
/// to answer a search box (task 0420 measured a 19x read amplification from
/// exactly that shape). Same goal, different access pattern, deliberately
/// different mechanism.
async fn search_pools_by_asset_code(
    client: &clickhouse::Client,
    q: &str,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    let codes = normalize_asset_codes(Some(q.to_string()));
    // An account- or contract-shaped query (a StrKey) names no asset code,
    // symbol or name. Without this gate it paid the scan below for a
    // guaranteed-empty result, and `/v1/search` runs its six buckets in
    // parallel, so that cost lands on the tail of every such search. (The gate
    // used to be "longer than a 12-character code", which since task 0636
    // would also drop a Soroban token's longer name.)
    if codes
        .iter()
        .any(|c| stellar_strkey::Strkey::from_string(c).is_ok())
    {
        return Ok(Vec::new());
    }
    let Some(filter) = pool_asset_filter(client, &codes).await? else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "SELECT pool_hex, pool_kind, legs \
         FROM ( \
            SELECT \
                lower(hex(pool_id)) AS pool_hex, \
                toInt16(argMax(pool_kind, last_updated_ledger)) AS pool_kind, \
                argMax(legs, last_updated_ledger) AS legs, \
                max(last_updated_ledger) AS newest \
            FROM liquidity_pools \
            GROUP BY pool_id \
         ) AS lp \
         WHERE {} \
         ORDER BY newest DESC \
         LIMIT ?",
        filter.sql
    );

    let mut query = client.query(&sql);
    for bind in &filter.binds {
        query = query.bind(bind);
    }
    for (name, ids) in &filter.params {
        query = query.param(name, ids);
    }
    let rows = query.bind(per_group_limit).fetch_all::<PoolRow>().await?;

    let leg_ids: BTreeSet<i64> = rows.iter().flat_map(|p| p.legs.iter().copied()).collect();
    let identities = resolve_asset_identities(client, &leg_ids).await?;
    rows.iter().map(|p| pool_hit(p, &identities)).collect()
}

/// Fires only for a hash-shaped query. `pool_id` is the full ORDER BY key, so
/// `pool_id = unhex(?)` is a granule-pruned point seek. Codes are version-stable
/// (composition is fixed; only reserves move `last_updated_ledger`), so
/// `ORDER BY last_updated_ledger DESC LIMIT 1` collapses re-ingest versions to
/// one row without a FINAL merge.
async fn search_pool_by_id(
    client: &clickhouse::Client,
    bytes: &[u8],
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    let hash_hex = hex::encode(bytes);

    let row = client
        .query(
            "SELECT lower(hex(pool_id)) AS pool_hex, \
                    toInt16(pool_kind) AS pool_kind, legs \
             FROM liquidity_pools \
             WHERE pool_id = unhex(?) \
             ORDER BY last_updated_ledger DESC \
             LIMIT 1",
        )
        .bind(&hash_hex)
        .fetch_optional::<PoolRow>()
        .await?;
    let Some(p) = row else {
        return Ok(Vec::new());
    };

    let leg_ids: BTreeSet<i64> = p.legs.iter().copied().collect();
    let identities = resolve_asset_identities(client, &leg_ids).await?;
    Ok(vec![pool_hit(&p, &identities)?])
}

/// Shared by both pool arms so an id hit and a code hit cannot describe the
/// same pool differently.
fn pool_hit(
    p: &PoolRow,
    identities: &HashMap<i64, ResolvedAsset>,
) -> Result<(String, SearchHit), clickhouse::error::Error> {
    let kind = decode_pool_kind(&p.pool_hex, p.pool_kind)?;
    Ok((
        "pool".to_string(),
        SearchHit {
            entity_type: EntityType::Pool,
            // The column projects raw hex; the wire form is chosen by the
            // pool's KIND at the boundary, because the same 32 bytes are an
            // `L…` strkey for a classic pool and a `C…` address for a soroban
            // one — and the wrong encoding is well-formed, not an error.
            identifier: pool_id_hex_to_strkey(&p.pool_hex, kind),
            label: p
                .legs
                .iter()
                .map(|id| leg_label(identities.get(id)))
                .collect::<Vec<_>>()
                .join(" / "),
            route_token: None,
            successful: None,
            last_activity_at: None,
            contract_id: None,
            token_id: None,
        },
    ))
}
