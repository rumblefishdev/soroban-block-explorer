//! A pool's total shares, from whichever source records them.

/// A classic pool's shares are protocol-scaled: `Decimal128(7)` in the
/// snapshot, read raw.
const CLASSIC_SHARE_DECIMALS: u32 = 7;

/// The pool's total shares as a RAW integer and its scale, or `None` when
/// unknown. The client scales, as for every other amount the API serves.
///
/// A CLASSIC pool's shares come from its snapshot (7 decimals, fixed by the
/// protocol). A SOROBAN pool has no snapshot — the table is classic-only — so
/// its shares come from `pool_instance_state`, which the indexer reads off the
/// pool contract's own storage, scaled by the share token's decimals (`None`
/// when its metadata does not say; the client then shows no number).
///
/// A soroban 0 is only sometimes a measurement — see [`zero_shares_is_measured`];
/// `pool_type_raw` and the pool's raw `state_reserves` decide it.
pub(super) fn pool_total_shares(
    snapshot_raw: Option<String>,
    instance_raw: Option<&str>,
    share_decimals: Option<u32>,
    pool_type_raw: &str,
    state_reserves: &[String],
) -> Option<(String, Option<u32>)> {
    if let Some(raw) = snapshot_raw {
        return Some((raw, Some(CLASSIC_SHARE_DECIMALS)));
    }
    match instance_raw? {
        "0" => zero_shares_is_measured(pool_type_raw, state_reserves)
            .then(|| ("0".to_string(), share_decimals)),
        raw => Some((raw.to_string(), share_decimals)),
    }
}

/// Whether a soroban pool's stored `total_shares = 0` is a MEASUREMENT.
///
/// The writer stores 0 for three different things (schema, `pool_instance_state`):
///   * **pair-factory** — every row carries the key, so 0 is a true zero. It is
///     the family that emits no pool type at all, so an empty `pool_type_raw`
///     identifies it (215 of 215 on production, 2026-09-09);
///   * **router** — 0 when the `TotalShares` key was ABSENT: structural for
///     concentrated and elastic pools, and for older contract versions that
///     have no `get_total_shares` at all;
///   * **config-factory** — 0 forever; its supply lives on the share token.
///
/// An EMPTY pool has no shares outstanding whatever its storage says, so a
/// pool whose every reserve is 0 reads 0 too. Measured 2026-09-24: 83 of 84
/// router constant-product pools and 39 of 39 stable pools with a stored 0 hold
/// nothing, and two sampled on chain answer `get_total_shares() = 0`; the one
/// constant-product pool that holds reserves is a contract without that
/// function. Everything else with a 0 reads as unknown, not as "no shares".
fn zero_shares_is_measured(pool_type_raw: &str, raw_reserves: &[String]) -> bool {
    pool_type_raw.is_empty() || (!raw_reserves.is_empty() && raw_reserves.iter().all(|r| r == "0"))
}

/// Each soroban pool's instance-state shares and its share token, for the
/// pools in `pool_ids` (the body of an `IN (…)`). The share token is a contract
/// surrogate, so its decimals come from the same identity resolution as the
/// legs (`common::asset_identity`) — one rule for "the scale is a fact".
///
/// `toNullable` on the shares: with `join_use_nulls = 0` an unmatched LEFT
/// JOIN yields the column DEFAULT, so a plain `String` would arrive as `''`
/// and refuse to decode into an `Option` (the task 0324 class). An unmatched
/// share token arrives as `0`, which resolves to nothing.
pub(super) fn instance_shares_sql(pool_ids: &str) -> String {
    format!(
        "SELECT pool_id, \
                toNullable(toString(argMax(total_shares, derived_at_ledger))) AS shares_raw, \
                argMax(share_token_id, derived_at_ledger) AS share_token_id \
         FROM pool_instance_state \
         WHERE pool_id IN ({pool_ids}) \
         GROUP BY pool_id"
    )
}

#[cfg(test)]
mod tests;
