//! What a pool holds of each leg, from whichever source records it.

use super::scale_decimal_str;

/// Leg `i`'s reserve in units, or `None` when it is not knowable.
///
/// A soroban pool's latest state change holds one RAW integer per leg, in leg
/// order (the pool's `get_tokens()` order, which `legs` keeps), scaled by that
/// leg's own decimals. `scale` is `None` when nothing established them: the
/// amount then has no value to show, since a guessed 7 is 10^11 off for an
/// 18-decimal token and still reads as a number. With no state change the
/// pool is classic, and its snapshot's two columns are already in units
/// (`Decimal128(7)`).
pub(super) fn leg_reserve(
    i: usize,
    state: &[String],
    snapshot: [Option<&str>; 2],
    scale: Option<u32>,
) -> Option<String> {
    if state.is_empty() {
        return snapshot.get(i).copied().flatten().map(str::to_string);
    }
    match state.get(i)?.as_str() {
        // Zero is zero at every scale, so an empty leg is knowable even when
        // its decimals are not.
        "0" => Some("0".to_string()),
        raw => scale_decimal_str(raw, scale?),
    }
}

/// Each soroban pool's latest reserves, raw and in leg order, for the pools in
/// `pool_ids` (the body of an `IN (…)`: `unhex(?)` or a subquery — bounded,
/// never the whole table). No ledger bound: a page of the busiest pools reads
/// 2.4M rows / ~57 ms (measured 2026-09-25) at a few dozen requests a day.
///
/// No plane filter either. Every row is staged from the pool's OWN instance,
/// keyed on the entry's owner (decision C′, `stage.rs`), so a contract can
/// only write rows under its own id. Before C′ rows came from a plane's
/// `[PoolData, pool]` key, which any contract could forge; on production
/// 0 of 5,040,494 rows come from a plane the pool does not declare
/// (2026-09-25). The newest row wins whatever plane wrote it, so a pool that
/// re-points its plane keeps reading its latest state.
pub(super) fn state_reserves_sql(pool_ids: &str) -> String {
    format!(
        "SELECT pool_id, \
                arrayMap(x -> toString(x), argMax(reserves, ledger_sequence)) AS reserves \
         FROM pool_state_changes \
         WHERE pool_id IN ({pool_ids}) \
         GROUP BY pool_id"
    )
}

#[cfg(test)]
mod tests;
