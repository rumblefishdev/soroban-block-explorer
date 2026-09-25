//! What a pool holds of each leg, from whichever source records it.

/// A pool's reserves, one RAW integer per leg, in leg order: a soroban pool's
/// latest state change when it has one (the pool's `get_tokens()` order, which
/// `legs` keeps), else the classic snapshot's two columns (read raw: the SQL
/// takes their `Decimal128(7)` × 10^7). Scaling is the client's, by each leg's
/// `decimals`, the same contract as every other amount the API serves.
pub(super) fn leg_reserves(
    state: &[String],
    snapshot_a: Option<&str>,
    snapshot_b: Option<&str>,
) -> Vec<Option<String>> {
    if state.is_empty() {
        vec![
            snapshot_a.map(str::to_string),
            snapshot_b.map(str::to_string),
        ]
    } else {
        state.iter().cloned().map(Some).collect()
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
