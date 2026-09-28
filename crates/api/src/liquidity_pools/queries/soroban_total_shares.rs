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

use crate::common::asset_identity::ResolvedAsset;

use super::soroban_reserves::scale_raw;

/// The subquery the list and detail reads LEFT JOIN on `pool_id` for a
/// soroban pool's stored total shares (`stored_shares`, as text) and its share
/// token (`share_token_id`); `{pools}` is the caller's pool-id predicate. A
/// classic pool has no row: the join miss reads `''` and `0`, which
/// [`StoredTotalShares::from_join`] turns into "no stored value".
///
/// The share token's scale is not joined here: the token is an asset like any
/// leg (730 of 730 share tokens are in `assets`), so the caller resolves it
/// with the legs and reads its decimals with theirs — see
/// [`share_token_decimals`].
pub(super) const STORED_SHARES_JOIN: &str = "SELECT pool_id, \
        toString(argMax(total_shares, derived_at_ledger)) AS stored_shares, \
        argMax(share_token_id, derived_at_ledger)         AS share_token_id \
     FROM pool_instance_state \
     WHERE {pools} \
     GROUP BY pool_id";

/// A share token's published decimals, from the identities and the token
/// decimals the caller resolved for the page; `None` for no share token (`0`,
/// a concentrated pool) or one that publishes none.
pub(super) fn share_token_decimals(
    share_token_id: i64,
    identities: &HashMap<i64, ResolvedAsset>,
    token_decimals: &HashMap<String, u32>,
) -> Option<u32> {
    let contract = identities
        .get(&share_token_id)?
        .contract_strkey
        .as_deref()?;
    token_decimals.get(contract).copied()
}

/// The stored total and the share token's decimals, per pool.
pub(super) struct StoredTotalShares {
    raw: String,
    decimals: Option<u32>,
}

impl StoredTotalShares {
    /// The pool's [`STORED_SHARES_JOIN`] columns, or `None` for a join miss
    /// (a pool with no `pool_instance_state` row reads an empty string).
    pub(super) fn from_join(raw: String, decimals: Option<u32>) -> Option<Self> {
        (!raw.is_empty()).then_some(Self { raw, decimals })
    }
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
