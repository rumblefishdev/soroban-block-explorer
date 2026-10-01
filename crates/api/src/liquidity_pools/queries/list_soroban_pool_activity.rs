//! `GET /v1/liquidity-pools/:id/activity` for a soroban pool — its
//! `pool_movements` rows folded into one row per operation, the same grain a
//! classic pool's activity has.

use clickhouse::Row;
use serde::Deserialize;

use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::liquidity_pools::dto::{PoolActivityCursor, PoolEvent};

use super::list_pool_activity::{ActivityOp, PoolActivityRow, enrich_activity};

/// One `pool_movements` row: one leg of one event.
#[derive(Debug, Row, Deserialize)]
struct MovementChRow {
    ls: i64,
    ao: i16,
    oi: u16,
    ei: u32,
    kind: u8,
    asset_id: i64,
    /// `Int128` read as text: a soroban leg may carry 18 decimals.
    amount: String,
}

/// One operation of the pool: every event it made the pool emit, folded.
/// `sums[i]` belongs to the pool's `legs[i]`; `None` when no event of the
/// operation carried that leg.
#[derive(Debug, PartialEq)]
struct SorobanOp {
    ls: i64,
    ao: i16,
    oi: u16,
    /// The stored `event_kind` of every event, `None` once two disagree.
    kind: Option<u8>,
    sums: Vec<Option<i128>>,
}

impl SorobanOp {
    /// The stored kind when every event of the operation agrees — a leg can
    /// be zero, so the signs alone cannot tell a one-sided deposit from a
    /// trade. An operation mixing kinds (a router depositing and swapping in
    /// one call) is named by the signs of its summed legs, as a classic one
    /// is; with a leg missing it has no event.
    fn event(&self) -> Option<PoolEvent> {
        match self.kind {
            Some(0) => Some(PoolEvent::Trade),
            Some(1) => Some(PoolEvent::Deposit),
            Some(2) => Some(PoolEvent::Withdrawal),
            Some(_) => None,
            None => {
                let signs: Vec<i64> = self
                    .sums
                    .iter()
                    .map(|s| s.map(|v| v.signum() as i64))
                    .collect::<Option<_>>()?;
                Some(PoolEvent::from_signs(&signs))
            }
        }
    }
}

/// Fold the key-ordered movement rows into operations.
///
/// Rows of one operation are adjacent: the sort key is `(pool_id,
/// ledger_sequence, application_order, operation_index, event_index,
/// asset_id)`, so a fold suffices. An unmerged duplicate has the same full
/// key as its neighbour and is skipped — the live writer and the backfill
/// write the same rows on purpose, so summing both would double the amount.
///
/// `truncated`: the read hit its row cap, so the last operation may be
/// missing rows that did not fit; it is dropped and re-read next round.
fn fold_movements(rows: Vec<MovementChRow>, legs: &[i64], truncated: bool) -> Vec<SorobanOp> {
    let mut out: Vec<SorobanOp> = Vec::new();
    let mut prev_key: Option<(i64, i16, u16, u32, i64)> = None;
    for r in rows {
        let key = (r.ls, r.ao, r.oi, r.ei, r.asset_id);
        if prev_key == Some(key) {
            continue;
        }
        prev_key = Some(key);
        let Ok(amount) = r.amount.parse::<i128>() else {
            continue;
        };

        let same_op = out
            .last()
            .is_some_and(|last| (last.ls, last.ao, last.oi) == (r.ls, r.ao, r.oi));
        if same_op {
            let op = out.last_mut().expect("checked above");
            if op.kind != Some(r.kind) {
                op.kind = None;
            }
        } else {
            out.push(SorobanOp {
                ls: r.ls,
                ao: r.ao,
                oi: r.oi,
                kind: Some(r.kind),
                sums: vec![None; legs.len()],
            });
        }
        // An asset that is not one of the pool's legs has no slot to land in.
        if let (Some(op), Some(i)) = (out.last_mut(), legs.iter().position(|&l| l == r.asset_id)) {
            op.sums[i] = Some(op.sums[i].unwrap_or(0) + amount);
        }
    }
    if truncated {
        out.pop();
    }
    out
}

/// Ledgers the first round reads. One partition of `pool_movements`
/// (`intDiv(ledger_sequence, 500000)`), so a busy pool's first page touches
/// one or two partitions.
const FIRST_SPAN: i64 = 500_000;

/// The soroban branch of `/activity`: one row per operation, newest first.
///
/// **Ledger window, not just `LIMIT`.** Reading the pool in reverse key order
/// with only a row limit still opens a granule in every partition the pool
/// has: on production's busiest pool (2.83M rows, 2026-10-01) that was ~210k
/// rows for a 64-row page, and a page past a deep cursor read the whole pool
/// (2.93M rows), because the tuple keyset does not prune the primary key.
/// Bounding `ledger_sequence` by plain numbers prunes it: 10.7k rows for the
/// first page, 53k for the deep one. So each round reads a ledger span ending
/// at the cursor; a span that runs out before the page fills is followed by
/// one twice as wide, until it passes ledger 0. A pool that has gone quiet
/// therefore costs a few cheap rounds, never one read of everything.
///
/// Inside a span the row cap works as in the classic branch: a page that
/// stops mid-operation drops that operation and resumes from the last
/// complete one, and the cap doubles so one huge operation still fits.
///
/// Amounts are raw token units, summed per leg over the operation's events;
/// the page scales them by each leg's `decimals`. `pools_crossed` is `None`:
/// a soroban operation's route is not in `transaction_operations.pool_ids`
/// (0 of 2,000 recent operations of the busiest pool carry one), and a `0`
/// there would claim the operation crossed nothing.
pub async fn fetch_soroban_pool_activity(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    legs: &[i64],
    limit: i64,
    cursor: Option<&PoolActivityCursor>,
    direction: Direction,
    event: Option<PoolEvent>,
) -> Result<Vec<PoolActivityRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);
    let newest_first = order == "DESC";

    // The fence the classic branch also keeps: nothing past the newest
    // committed ledger, so the transaction join below always resolves.
    let tip: i64 = client
        .query("SELECT max(sequence) FROM ledgers")
        .fetch_one::<i64>()
        .await?;

    let mut after: Option<(i64, i16, u16)> = cursor.map(|c| {
        (
            c.ledger_sequence,
            c.application_order,
            c.operation_index as u16,
        )
    });
    // The span `(lo, hi]` the current round reads.
    let mut span = FIRST_SPAN;
    let (mut lo, mut hi) = match (newest_first, after) {
        (true, Some((ls, _, _))) => (ls - span, ls.min(tip)),
        (true, None) => (tip - span, tip),
        (false, Some((ls, _, _))) => (ls - 1, (ls - 1 + span).min(tip)),
        (false, None) => (-1, span.min(tip)),
    };

    let legs_per_op = legs.len().max(1) as i64;
    let mut row_cap = (limit * legs_per_op * 2).max(256);
    let mut ops: Vec<SorobanOp> = Vec::new();

    loop {
        let keyset = match after {
            Some((ls, ao, oi)) => format!(
                " AND (ledger_sequence, application_order, operation_index) {op} ({ls}, {ao}, {oi})"
            ),
            None => String::new(),
        };
        let sql = format!(
            "SELECT \
                ledger_sequence      AS ls, \
                application_order    AS ao, \
                operation_index      AS oi, \
                event_index          AS ei, \
                event_kind           AS kind, \
                asset_id             AS asset_id, \
                toString(amount)     AS amount \
             FROM pool_movements \
             WHERE pool_id = toFixedString(unhex(?), 32) \
               AND ledger_sequence > {lo} AND ledger_sequence <= {hi} {keyset} \
             ORDER BY ls {order}, ao {order}, oi {order}, ei {order}, asset_id {order} \
             LIMIT {row_cap}"
        );
        let rows = client
            .query(&sql)
            .bind(pool_id_hex)
            .fetch_all::<MovementChRow>()
            .await?;

        let truncated = rows.len() as i64 >= row_cap;
        let batch = fold_movements(rows, legs, truncated);
        if let Some(last) = batch.last() {
            after = Some((last.ls, last.ao, last.oi));
        }
        match event {
            Some(want) => ops.extend(batch.into_iter().filter(|o| o.event() == Some(want))),
            None => ops.extend(batch),
        }
        if ops.len() as i64 >= limit {
            break;
        }

        if truncated {
            // More of this span is left; read on from the last complete
            // operation with room for a bigger one.
            row_cap *= 2;
            continue;
        }
        // This span is spent: the next one, twice as wide.
        if newest_first {
            if lo < 0 {
                break;
            }
            span *= 2;
            hi = lo;
            lo = hi - span;
        } else {
            if hi >= tip {
                break;
            }
            span *= 2;
            lo = hi;
            hi = (lo + span).min(tip);
        }
    }
    ops.truncate(limit as usize);

    let ops = ops
        .into_iter()
        .map(|o| ActivityOp {
            ls: o.ls,
            ao: o.ao,
            oi: o.oi as i16,
            event: o.event(),
            amounts: o.sums.iter().map(|s| s.map(|v| v.to_string())).collect(),
        })
        .collect();
    let mut rows = enrich_activity(client, ops).await?;
    for r in &mut rows {
        r.pools_crossed = None;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
