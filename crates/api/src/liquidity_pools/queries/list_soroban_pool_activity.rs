//! `GET /v1/liquidity-pools/:id/activity` for a soroban pool — one row per
//! event the pool emitted, read from `pool_movements`.

use clickhouse::Row;
use db_clickhouse::persist::stage::soroban_pool_amounts::PoolEventKind;
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

/// One event the pool emitted, with its legs. `amounts[i]` belongs to the
/// pool's `legs[i]`; `None` when the event did not name that leg.
#[derive(Debug, PartialEq)]
struct PoolMovement {
    ls: i64,
    ao: i16,
    oi: u16,
    ei: u32,
    kind: u8,
    amounts: Vec<Option<i128>>,
}

impl PoolMovement {
    /// The kind the pool's own event declared — stored, because a leg can be
    /// zero and the signs alone would call a one-sided deposit a trade.
    fn event(&self) -> Option<PoolEvent> {
        Some(match PoolEventKind::from_stored(self.kind)? {
            PoolEventKind::Trade => PoolEvent::Trade,
            PoolEventKind::Deposit => PoolEvent::Deposit,
            PoolEventKind::Withdrawal => PoolEvent::Withdrawal,
        })
    }
}

/// The stored `event_kind` a `filter[event]` value selects.
fn stored_kind(event: PoolEvent) -> u8 {
    match event {
        PoolEvent::Trade => PoolEventKind::Trade as u8,
        PoolEvent::Deposit => PoolEventKind::Deposit as u8,
        PoolEvent::Withdrawal => PoolEventKind::Withdrawal as u8,
    }
}

/// Group the key-ordered rows into events.
///
/// Rows of one event are adjacent: the sort key is `(pool_id,
/// ledger_sequence, application_order, operation_index, event_index,
/// asset_id)`. An unmerged duplicate has the same full key as its neighbour
/// and is skipped — the live writer and the backfill write the same rows on
/// purpose.
///
/// `truncated`: the read hit its row cap, so the last event may be missing
/// legs that did not fit; it is dropped and re-read next round.
fn group_movements(rows: Vec<MovementChRow>, legs: &[i64], truncated: bool) -> Vec<PoolMovement> {
    let mut out: Vec<PoolMovement> = Vec::new();
    let mut prev_key: Option<(i64, i16, u16, u32, i64)> = None;
    for r in rows {
        let key = (r.ls, r.ao, r.oi, r.ei, r.asset_id);
        if prev_key == Some(key) {
            continue;
        }
        prev_key = Some(key);

        let same_event = out
            .last()
            .is_some_and(|last| (last.ls, last.ao, last.oi, last.ei) == (r.ls, r.ao, r.oi, r.ei));
        if !same_event {
            out.push(PoolMovement {
                ls: r.ls,
                ao: r.ao,
                oi: r.oi,
                ei: r.ei,
                kind: r.kind,
                amounts: vec![None; legs.len()],
            });
        }
        // `toString(Int128)` always parses; a row that does not is a decode
        // fault worth seeing, and its leg stays unknown.
        let Ok(amount) = r.amount.parse::<i128>() else {
            tracing::error!(ls = r.ls, ao = r.ao, oi = r.oi, ei = r.ei, amount = %r.amount, "unparseable pool_movements amount");
            continue;
        };
        // An asset that is not one of the pool's legs has no slot to land in.
        if let (Some(ev), Some(i)) = (out.last_mut(), legs.iter().position(|&l| l == r.asset_id)) {
            ev.amounts[i] = Some(amount);
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

/// The soroban branch of `/activity`: one row per event the pool emitted,
/// newest first.
///
/// **An event, not an operation.** `pool_movements` is event-grained, and the
/// event is what the pool itself declared. Summing an operation's events made
/// rows that described nothing: an operation withdrawing and re-depositing
/// netted to a one-unit "trade" (688 such operations on production,
/// 2026-10-01), and several swaps in one call could net to `+/+` under a
/// Trade chip. A classic operation is one movement, so both kinds still list
/// what moved through the pool, each row named by its own kind; the filter is
/// therefore exact in SQL.
///
/// **Ledger window, not just `LIMIT`.** Reading the pool in reverse key order
/// with only a row limit still opens a granule in every partition the pool
/// has: on production's busiest pool (2.83M rows, 2026-10-01) that was ~210k
/// rows for a 64-row page, and a page past a deep cursor read the whole pool
/// (2.93M rows), because the tuple keyset does not prune the primary key.
/// Bounding `ledger_sequence` by plain numbers prunes it: 22.6k rows for the
/// first page, 16k for the deep one. So each round reads a ledger span ending
/// at the cursor; a span that runs out before the page fills is followed by
/// one twice as wide, until it passes ledger 0. A pool that has gone quiet
/// therefore costs a few cheap rounds, never one read of everything.
///
/// Inside a span the row cap works as in the classic branch: a page that
/// stops mid-event drops that event and resumes from the last complete one,
/// and the cap doubles so a long run of non-matching rows is crossed quickly.
///
/// Amounts are raw token units; the page scales them by each leg's
/// `decimals`. `pools_crossed` is `None`: a soroban operation's route is not
/// in `transaction_operations.pool_ids` (0 of 2,000 recent operations of the
/// busiest pool carry one), and a `0` there would claim it crossed nothing.
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

    let mut after: Option<(i64, i16, u16, u32)> = cursor.map(|c| {
        (
            c.ledger_sequence,
            c.application_order,
            c.operation_index as u16,
            c.event_index,
        )
    });
    // The span `(lo, hi]` the current round reads.
    let mut span = FIRST_SPAN;
    let (mut lo, mut hi) = match (newest_first, after) {
        (true, Some((ls, ..))) => (ls - span, ls.min(tip)),
        (true, None) => (tip - span, tip),
        (false, Some((ls, ..))) => (ls - 1, (ls - 1 + span).min(tip)),
        (false, None) => (-1, span.min(tip)),
    };

    let legs_per_event = legs.len().max(1) as i64;
    let mut row_cap = (limit * legs_per_event * 2).max(256);
    let kind_filter = match event {
        Some(want) => format!(" AND event_kind = {}", stored_kind(want)),
        None => String::new(),
    };
    let mut movements: Vec<PoolMovement> = Vec::new();

    loop {
        let keyset = match after {
            Some((ls, ao, oi, ei)) => format!(
                " AND (ledger_sequence, application_order, operation_index, event_index) \
                 {op} ({ls}, {ao}, {oi}, {ei})"
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
               AND ledger_sequence > {lo} AND ledger_sequence <= {hi}{kind_filter}{keyset} \
             ORDER BY ls {order}, ao {order}, oi {order}, ei {order}, asset_id {order} \
             LIMIT {row_cap}"
        );
        let rows = client
            .query(&sql)
            .bind(pool_id_hex)
            .fetch_all::<MovementChRow>()
            .await?;

        let truncated = rows.len() as i64 >= row_cap;
        let batch = group_movements(rows, legs, truncated);
        if let Some(last) = batch.last() {
            after = Some((last.ls, last.ao, last.oi, last.ei));
        }
        movements.extend(batch);
        if movements.len() as i64 >= limit {
            break;
        }

        if truncated {
            // More of this span is left; read on from the last complete
            // event with a bigger cap.
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
    movements.truncate(limit as usize);

    let ops = movements
        .into_iter()
        .map(|m| ActivityOp {
            ls: m.ls,
            ao: m.ao,
            oi: m.oi as i16,
            event_index: Some(m.ei),
            event: m.event(),
            amounts: m.amounts.iter().map(|a| a.map(|v| v.to_string())).collect(),
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
