//! A pool's total shares, from whichever source records them.

use super::leg_reserves::scale_decimal_str;

/// The pool's total shares as a decimal string, or `None` when unknown.
///
/// A CLASSIC pool's shares come from its snapshot, already scaled by the
/// column's `Decimal128(7)`. A SOROBAN pool has no snapshot — the table is
/// classic-only — so its shares come from `pool_instance_state`, which the
/// indexer reads off the pool contract's own storage: a RAW integer, scaled by
/// the share token's decimals (`None` when its metadata does not say).
///
/// A soroban 0 is only sometimes a measurement — see [`zero_shares_is_measured`].
pub(super) fn total_shares_of(
    snapshot: Option<String>,
    instance_raw: Option<&str>,
    share_decimals: Option<u32>,
    zero_is_measured: bool,
) -> Option<String> {
    if snapshot.is_some() {
        return snapshot;
    }
    match instance_raw? {
        "0" => zero_is_measured.then(|| "0".to_string()),
        raw => scale_decimal_str(raw, share_decimals?),
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
pub(super) fn zero_shares_is_measured(pool_type_raw: &str, raw_reserves: &[String]) -> bool {
    pool_type_raw.is_empty() || (!raw_reserves.is_empty() && raw_reserves.iter().all(|r| r == "0"))
}

/// Each soroban pool's instance-state shares and its share token's decimals,
/// for the pools in `pool_ids` (the body of an `IN (…)` — bounded, never whole).
///
/// `toNullable` on the projected columns: with `join_use_nulls = 0` an
/// unmatched LEFT JOIN yields the column DEFAULT, so a plain `String` would
/// arrive as `''` and refuse to decode into an `Option` (the task 0324 class).
/// Decimals stay NULL when the share token publishes none — never a guessed 7.
pub(super) fn instance_shares_sql(pool_ids: &str) -> String {
    format!(
        "SELECT s.pool_id AS pool_id, \
                toNullable(toString(s.total_shares)) AS shares_raw, \
                CAST(m.decimals AS Nullable(UInt32)) AS shares_decimals \
         FROM (SELECT pool_id, \
                      argMax(share_token_id, derived_at_ledger) AS share_token_id, \
                      argMax(total_shares, derived_at_ledger) AS total_shares \
               FROM pool_instance_state \
               WHERE pool_id IN ({pool_ids}) \
               GROUP BY pool_id) s \
         LEFT JOIN (SELECT id, contract_id FROM soroban_contracts LIMIT 1 BY id) c \
             ON c.id = s.share_token_id \
         LEFT JOIN (SELECT contract_id, toNullable(argMax(decimals, version)) AS decimals \
                    FROM soroban_contract_metadata GROUP BY contract_id) m \
             ON m.contract_id = c.contract_id"
    )
}

#[cfg(test)]
mod tests;
