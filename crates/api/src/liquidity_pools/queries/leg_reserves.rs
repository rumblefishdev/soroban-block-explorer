//! What a pool holds of each leg, from whichever source records it.

/// The largest scale treated as a fact. A `u128` has 39 digits, so no real
/// token needs more; a larger value is broken or hostile metadata (two live
/// contracts declare 43,224) and would otherwise size the padding below.
const MAX_SCALE: u32 = 38;

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

impl Reserves<'_> {
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

/// A raw integer amount as a decimal string, scaled by `decimals`.
///
/// STRING SURGERY, not arithmetic: the value is a `u128` out of contract
/// storage, an `f64` drops digits above 2^53, and a `Decimal128` division would
/// have to pick its scale up front. Inserting the point is exact at every
/// magnitude.
pub(super) fn scale_decimal_str(raw: &str, decimals: u32) -> Option<String> {
    let digits = raw.strip_prefix('+').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || decimals > MAX_SCALE {
        return None;
    }
    let d = decimals as usize;
    if d == 0 {
        return Some(digits.to_string());
    }
    // Left-pad so there is always at least one integer digit.
    let padded = format!("{digits:0>width$}", width = d + 1);
    let split = padded.len() - d;
    let frac = padded[split..].trim_end_matches('0');
    Some(if frac.is_empty() {
        padded[..split].to_string()
    } else {
        format!("{}.{}", &padded[..split], frac)
    })
}

/// Each soroban pool's latest reserves, raw and in leg order, for the pools in
/// `pool_ids` (the body of an `IN (…)`: `unhex(?)` or a subquery — bounded,
/// never the whole table). It appears twice, so a bound value binds twice.
/// `from_ledger` is a lower bound on the rows read (an SQL expression): the
/// list passes its page's oldest activity, which no page pool's latest row can
/// precede — 0.25M rows read instead of 2.66M for a page of the busiest pools
/// (measured 2026-09-24) — and the single-pool detail passes `0`.
///
/// **The plane filter is required, not an optimisation.** A plane entry names
/// its pool in a key payload the writing contract chooses freely, so any
/// contract can publish rows under another pool's id. Only rows from the plane
/// the pool itself declares in `pool_instance_state` — the pool contract's own
/// storage, which nobody else can write — are its reserves. Filtering at write
/// time is not possible: under a parallel backfill a pool's reserve rows can
/// land before its declaration (task 0374). Every read of `pool_state_changes`
/// carries this join.
pub(super) fn state_reserves_sql(pool_ids: &str, from_ledger: &str) -> String {
    format!(
        "SELECT s.pool_id AS pool_id, \
                arrayMap(x -> toString(x), argMax(s.reserves, s.ledger_sequence)) AS reserves \
         FROM pool_state_changes AS s \
         INNER JOIN (SELECT pool_id, argMax(plane_id, derived_at_ledger) AS plane_id \
                     FROM pool_instance_state \
                     WHERE pool_id IN ({pool_ids}) \
                     GROUP BY pool_id) AS d \
             ON d.pool_id = s.pool_id AND d.plane_id = s.plane_id \
         WHERE s.pool_id IN ({pool_ids}) AND s.ledger_sequence >= {from_ledger} \
         GROUP BY s.pool_id"
    )
}

#[cfg(test)]
mod tests;
