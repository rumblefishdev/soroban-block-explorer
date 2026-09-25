//! What a pool holds of each leg, from whichever source records it.

use super::scale_decimal_str;

/// Where a pool's per-leg reserves come from, in the source's own units.
pub(super) enum Reserves<'a> {
    /// A classic pool's snapshot: exactly two, already scaled by the column's
    /// `Decimal128(7)`.
    Pair(Option<&'a str>, Option<&'a str>),
    /// A soroban pool's latest state change: one raw integer per leg, in leg
    /// order (the pool's `get_tokens()` order, which `legs` keeps), to be
    /// scaled by each leg's own decimals.
    Raw(&'a [String]),
}

impl<'a> Reserves<'a> {
    /// A soroban pool's state-change reserves when it has any, else the
    /// classic snapshot pair.
    pub(super) fn from_sources(
        state: &'a [String],
        snapshot_a: Option<&'a str>,
        snapshot_b: Option<&'a str>,
    ) -> Self {
        if state.is_empty() {
            Self::Pair(snapshot_a, snapshot_b)
        } else {
            Self::Raw(state)
        }
    }

    /// The reserve for leg `i` in units, or `None` when it is not knowable.
    ///
    /// `scale` is `None` when nothing established the leg's decimals. A raw
    /// amount then has NO renderable value: a guessed 7 is wrong by up to
    /// 10^11 for an 18-decimal token and still reads as a number.
    pub(super) fn at(&self, i: usize, scale: Option<u32>) -> Option<String> {
        match self {
            Self::Pair(a, b) => match i {
                0 => a.map(str::to_string),
                1 => b.map(str::to_string),
                // The snapshot is pair-shaped; a third leg has no slot in it.
                _ => None,
            },
            Self::Raw(v) => match v.get(i)?.as_str() {
                // Zero is zero at every scale, so an empty leg is knowable even
                // when its decimals are not.
                "0" => Some("0".to_string()),
                raw => scale_decimal_str(raw, scale?),
            },
        }
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
