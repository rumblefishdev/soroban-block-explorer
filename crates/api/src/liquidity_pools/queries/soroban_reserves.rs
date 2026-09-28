//! A soroban pool's leg reserves — the newest `pool_state_changes` row per
//! pool, one raw `Int128` per leg.
//!
//! The vector is in the pool's own token order, which is the order
//! `liquidity_pools.legs` stores. Checked on chain, 2026-09-25: 16 of 16
//! pools across every family and type (router constant / stable 2- and
//! 3-leg / concentrated / elastic, pair-factory, config-factory) list their
//! tokens in our leg order, and 15 of 16 hold exactly our newest reserves (the
//! 16th traded after our row).
//!
//! Only a leg whose scale is a fact is served: native XLM and a classic credit
//! asset (reached through its SAC) have 7 decimals by protocol; a soroban
//! token's scale is the `decimals` its contract publishes in its metadata
//! (6, 7, 8, 9 and 18 occur). A token that publishes none — 4 of 100 soroban
//! legs on production, 2026-09-28 — keeps its leg `None`, never a raw integer
//! that would read as a huge amount and never an assumed 7. Checked on
//! chain: 13 of 13 published values equal the token's own `decimals()`.

use std::collections::HashMap;

use clickhouse::Row;
use serde::Deserialize;

use crate::common::asset_identity::ResolvedAsset;

#[derive(Debug, Row, Deserialize)]
struct TokenDecimalsChRow {
    contract_id: String,
    decimals: Option<u32>,
}

/// The contract address of every soroban-token leg among `leg_ids`.
pub(super) fn soroban_token_contracts<'a>(
    leg_ids: impl IntoIterator<Item = &'a i64>,
    identities: &'a HashMap<i64, ResolvedAsset>,
) -> Vec<&'a str> {
    let mut out: Vec<&str> = leg_ids
        .into_iter()
        .filter_map(|id| identities.get(id))
        .filter(|r| r.known && r.asset_type == domain::AssetFamily::Soroban as i16)
        .filter_map(|r| r.contract_strkey.as_deref())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Published `decimals` per token contract. A token with no metadata row, or
/// whose newest row carries no `decimals`, is absent from the map. The value is
/// wrapped in a tuple because `argMax` skips a `NULL` argument and would
/// return an older version's decimals instead.
pub(super) async fn fetch_token_decimals(
    client: &clickhouse::Client,
    contracts: &[&str],
) -> Result<HashMap<String, u32>, clickhouse::error::Error> {
    // Contract StrKeys are base32 (A-Z, 2-7); anything else is not inlined.
    let ids: Vec<&str> = contracts
        .iter()
        .copied()
        .filter(|c| {
            c.len() == 56
                && c.bytes()
                    .all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
        })
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_list = ids
        .iter()
        .map(|c| format!("'{c}'"))
        .collect::<Vec<_>>()
        .join(",");
    let rows = client
        .query(&format!(
            "SELECT contract_id, argMax(tuple(decimals), version).1 AS decimals \
             FROM soroban_contract_metadata \
             WHERE contract_id IN ({in_list}) \
             GROUP BY contract_id"
        ))
        .fetch_all::<TokenDecimalsChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| Some((r.contract_id, r.decimals?)))
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct ReservesChRow {
    pool_id_hex: String,
    reserves: Vec<String>,
}

/// Registered pools whose contract code is no longer a pool, as lowercase
/// pool-id hex. Their newest state row is the last one the pool code wrote,
/// not what the contract holds now, so no reserve is served for them.
///
/// `CAZ6W4WH…`: code replaced at ledger 54,515,539 (2024-11-22) and its
/// balances moved out at 63,767,534 (2026-08-02); our newest row, from
/// 54,514,504, still reads 26,351 PHO and 13,194 USDC.
// ponytail: a hand-kept list, one entry, measured 2026-09-28 (774 of 775
// pools equal their own storage). Task 0325 replaces it with a verdict
// written when a contract's code changes.
const NOT_A_POOL: &[&str] = &["33eb72c7a9a01352389d1cb151ce3dbd5818007c9e21ff5520f2c085f8f9f9b0"];

/// Newest raw reserves per pool, keyed by lowercase pool-id hex. A pool in
/// [`NOT_A_POOL`] gets no entry.
///
/// No plane filter: every row is decoded from the pool's own instance and
/// keyed on the entry's owner (decision C′), and production holds no row off
/// the pool's declared plane. Unmerged duplicates of one `(pool, plane,
/// ledger)` key carry identical values, so `argMax` is exact over them.
pub(super) async fn fetch_raw_reserves(
    client: &clickhouse::Client,
    pool_ids_hex: &[&str],
) -> Result<HashMap<String, Vec<String>>, clickhouse::error::Error> {
    // The ids come from our own `lower(hex(pool_id))` projection; the filter
    // keeps the inlined literal safe even if a caller ever passes user input.
    let ids: Vec<&str> = pool_ids_hex
        .iter()
        .copied()
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .filter(|h| !NOT_A_POOL.contains(h))
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_list = ids
        .iter()
        .map(|h| format!("unhex('{h}')"))
        .collect::<Vec<_>>()
        .join(",");
    let rows = client
        .query(&format!(
            "SELECT lower(hex(pool_id)) AS pool_id_hex, \
                    arrayMap(x -> toString(x), argMax(reserves, ledger_sequence)) AS reserves \
             FROM pool_state_changes \
             WHERE pool_id IN ({in_list}) \
             GROUP BY pool_id"
        ))
        .fetch_all::<ReservesChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.pool_id_hex, r.reserves))
        .collect())
}

/// One reserve per leg, scaled by the leg's known decimals; `None` for a leg
/// whose scale is not a fact, for a leg with no raw value, and for an
/// unparseable one.
pub(super) fn leg_reserves(
    leg_ids: &[i64],
    identities: &HashMap<i64, ResolvedAsset>,
    token_decimals: &HashMap<String, u32>,
    raw: &[String],
) -> Vec<Option<String>> {
    leg_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let decimals =
                identities
                    .get(id)
                    .filter(|r| r.known)
                    .and_then(|r| match r.asset_type {
                        t if t == domain::AssetFamily::Native as i16
                            || t == domain::AssetFamily::ClassicCredit as i16 =>
                        {
                            Some(7)
                        }
                        t if t == domain::AssetFamily::Soroban as i16 => r
                            .contract_strkey
                            .as_deref()
                            .and_then(|c| token_decimals.get(c).copied()),
                        _ => None,
                    })?;
            raw.get(i).and_then(|v| scale_raw(v, decimals))
        })
        .collect()
}

/// A raw integer amount scaled by `decimals`, as a decimal string with
/// trailing zeros trimmed — the shape ClickHouse gives a classic reserve
/// (`Decimal128(7)`), so both kinds of pool read the same on the wire.
pub(super) fn scale_raw(raw: &str, decimals: u32) -> Option<String> {
    let v: i128 = raw.trim().parse().ok()?;
    let sign = if v < 0 { "-" } else { "" };
    let abs = v.unsigned_abs();
    let unit = 10u128.checked_pow(decimals)?;
    let whole = abs / unit;
    let frac = abs % unit;
    if frac == 0 {
        return Some(format!("{sign}{whole}"));
    }
    let frac = format!("{frac:0width$}", width = decimals as usize);
    Some(format!("{sign}{whole}.{}", frac.trim_end_matches('0')))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
