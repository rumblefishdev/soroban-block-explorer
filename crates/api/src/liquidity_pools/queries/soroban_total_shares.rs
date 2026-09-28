//! A soroban pool's total shares — read from the pool's own instance storage
//! (`pool_instance_state.total_shares`), scaled by its share token's
//! decimals.
//!
//! The stored value is `0` both when the pool holds nothing and when its
//! instance has no `TotalShares` key at all: a concentrated pool (no share
//! token) and a config-factory pool (its supply lives on the separate share
//! token) always store `0`. So `0` is served only where it was measured —
//! a pool whose every reserve is `0`. Production, 2026-09-25: 575 pools hold
//! a positive total, 133 empty pools store `0`, and 66 pools holding reserves
//! store `0` (48 concentrated, 17 config-factory, 1 old-code constant pool);
//! 15 of 15 sampled values, positive and zero, equal the chain's
//! `get_total_shares()` / `total_supply()`. Every share token has published
//! decimals (7, all 726); a token without them reads `null`, never a guess.

use std::collections::HashMap;

use clickhouse::Row;
use serde::Deserialize;

use super::soroban_reserves::scale_raw;
use crate::common::contract_metadata::CONTRACT_METADATA;

#[derive(Debug, Row, Deserialize)]
struct TotalSharesChRow {
    pool_id_hex: String,
    total_shares: String,
    decimals: Option<u32>,
}

/// The stored total and the share token's decimals, per pool.
pub(super) struct StoredTotalShares {
    raw: String,
    decimals: Option<u32>,
}

/// Newest stored total shares per pool, keyed by lowercase pool-id hex.
pub(super) async fn fetch_total_shares(
    client: &clickhouse::Client,
    pool_ids_hex: &[&str],
) -> Result<HashMap<String, StoredTotalShares>, clickhouse::error::Error> {
    let ids: Vec<&str> = pool_ids_hex
        .iter()
        .copied()
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_list = ids
        .iter()
        .map(|h| format!("unhex('{h}')"))
        .collect::<Vec<_>>()
        .join(",");
    // `decimals` is `Nullable(UInt32)`, so a pool whose share token or its
    // metadata row is missing reads NULL through both LEFT JOINs — not a
    // default `0` that would scale the total by 10^0.
    let rows = client
        .query(&format!(
            "SELECT lower(hex(i.pool_id))   AS pool_id_hex, \
                    toString(i.ts)          AS total_shares, \
                    m.decimals              AS decimals \
             FROM ( \
                 SELECT pool_id, \
                        argMax(total_shares, derived_at_ledger)   AS ts, \
                        argMax(share_token_id, derived_at_ledger) AS token_id \
                 FROM pool_instance_state \
                 WHERE pool_id IN ({in_list}) \
                 GROUP BY pool_id \
             ) i \
             LEFT JOIN ( \
                 SELECT id, contract_id FROM soroban_contracts \
                 WHERE id IN (SELECT argMax(share_token_id, derived_at_ledger) \
                              FROM pool_instance_state \
                              WHERE pool_id IN ({in_list}) GROUP BY pool_id) \
                 LIMIT 1 BY id \
             ) sc ON sc.id = i.token_id \
             LEFT JOIN {CONTRACT_METADATA} m ON m.contract_id = sc.contract_id"
        ))
        .fetch_all::<TotalSharesChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.pool_id_hex,
                StoredTotalShares {
                    raw: r.total_shares,
                    decimals: r.decimals,
                },
            )
        })
        .collect())
}

/// The total shares to serve: the stored value scaled by the share token's
/// decimals, `"0"` only for a pool whose every reserve is `0`, `null` when the
/// stored `0` means "no such key" or the scale is unknown.
pub(super) fn served_total_shares(
    stored: Option<&StoredTotalShares>,
    raw_reserves: &[String],
) -> Option<String> {
    let stored = stored?;
    let decimals = stored.decimals?;
    let total: i128 = stored.raw.trim().parse().ok()?;
    if total != 0 {
        return scale_raw(&stored.raw, decimals);
    }
    let empty = !raw_reserves.is_empty()
        && raw_reserves
            .iter()
            .all(|r| r.trim().parse::<i128>().is_ok_and(|v| v == 0));
    empty.then(|| "0".to_string())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
