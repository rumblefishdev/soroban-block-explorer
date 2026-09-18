//! Statement B of `GET /v1/transactions` — the transactions that touch a
//! contract — by transaction position (task 0541).
//!
//! Three arms say a transaction touches a contract: its events, its
//! invocations, its operations. None of them sorts by transaction position, so
//! one SQL `UNION … ORDER BY … LIMIT` over them hashes the whole partition
//! first (6.04 GiB for native XLM, over the 4 GB reader cap — measured in the
//! task's phase 1). Instead each arm reads a window bounded by ledger — its
//! first `window` rows past the cursor, then every transaction of the ledgers
//! that window reached — and the windows merge here, in execution order.
//!
//! Correctness: an arm cut short by its window (`truncated`) is complete only
//! as far as its `last_ledger`, so the merged page keeps only positions on the
//! complete side of the most restrictive truncated arm. Too few left → the
//! window doubles.

use clickhouse::Row;
use serde::Deserialize;

use crate::common::cursor::{Direction, keyset_sql_desc};

/// A transaction's position in the chain: `(ledger_sequence, application_order)`.
pub type Position = (i64, i16);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmWindow {
    /// Positions past the cursor, complete for every ledger the window reached.
    pub positions: Vec<Position>,
    /// The arm had at least `window` rows past the cursor.
    pub truncated: bool,
    /// The ledger of the arm's `window`-th row; `None` unless `truncated`.
    pub last_ledger: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MergeResult {
    Page(Vec<Position>),
    /// Fewer than `take` positions on the complete side; carries them.
    NeedWiderWindow(Vec<Position>),
}

// ponytail: bounded doubling (window × 32); a contract sparse enough to need
// more gets a short page (logged), which ends its pagination early.
const MAX_ROUNDS: u32 = 6;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Arm {
    Events,
    Invocations,
    Operations,
}

impl Arm {
    fn table(self) -> &'static str {
        match self {
            Arm::Events => "soroban_events",
            Arm::Invocations => "soroban_invocations_appearances",
            Arm::Operations => "operations_appearances",
        }
    }

    /// How the arm names the transaction: events by position, the others by
    /// the `transactions.id` surrogate.
    fn tx_column(self) -> &'static str {
        match self {
            Arm::Events => "application_order",
            Arm::Invocations | Arm::Operations => "transaction_id",
        }
    }
}

fn descending(direction: Direction) -> bool {
    keyset_sql_desc(direction).1 == "DESC"
}

pub fn merge_arm_windows(arms: &[ArmWindow], direction: Direction, take: usize) -> MergeResult {
    let desc = descending(direction);
    let cap = arms
        .iter()
        .filter(|a| a.truncated)
        .filter_map(|a| a.last_ledger)
        .reduce(|a, b| if desc { a.max(b) } else { a.min(b) });
    let mut positions: Vec<Position> = arms
        .iter()
        .flat_map(|a| a.positions.iter().copied())
        .filter(|&(ledger, _)| match cap {
            Some(c) if desc => ledger >= c,
            Some(c) => ledger <= c,
            None => true,
        })
        .collect();
    positions.sort_unstable();
    positions.dedup();
    if desc {
        positions.reverse();
    }
    let enough = positions.len() >= take || !arms.iter().any(|a| a.truncated);
    positions.truncate(take);
    if enough {
        MergeResult::Page(positions)
    } else {
        MergeResult::NeedWiderWindow(positions)
    }
}

/// The ledger of the arm's `window`-th row from the cursor's ledger on, if the
/// arm has that many. Reads in key order: every arm's key leads with
/// `ledger_sequence` after the (constant) contract.
pub(crate) fn window_last_ledger_sql(
    arm: Arm,
    contract_id: i64,
    partition: &str,
    head_max: &str,
    cursor_ledger: Option<i64>,
    direction: Direction,
    window: usize,
) -> String {
    let (op, order) = keyset_sql_desc(direction);
    let cursor =
        cursor_ledger.map_or_else(String::new, |l| format!(" AND ledger_sequence {op}= {l}"));
    format!(
        "SELECT ledger_sequence FROM {table} \
         WHERE contract_id = {contract_id} AND {partition} \
           AND ledger_sequence <= {head_max}{cursor} \
         ORDER BY ledger_sequence {order} \
         LIMIT 1 OFFSET {offset}",
        table = arm.table(),
        offset = window.saturating_sub(1),
    )
}

/// Every transaction the arm names from the cursor's ledger to `last_ledger`,
/// both inclusive (the exact cursor position is applied in Rust).
pub(crate) fn window_rows_sql(
    arm: Arm,
    contract_id: i64,
    partition: &str,
    head_max: &str,
    cursor_ledger: Option<i64>,
    last_ledger: Option<i64>,
    direction: Direction,
) -> String {
    let (op, _) = keyset_sql_desc(direction);
    let back = if op == "<" { ">" } else { "<" };
    let cursor =
        cursor_ledger.map_or_else(String::new, |l| format!(" AND ledger_sequence {op}= {l}"));
    let last =
        last_ledger.map_or_else(String::new, |l| format!(" AND ledger_sequence {back}= {l}"));
    format!(
        "SELECT DISTINCT ledger_sequence, {column} FROM {table} \
         WHERE contract_id = {contract_id} AND {partition} \
           AND ledger_sequence <= {head_max}{cursor}{last}",
        column = arm.tx_column(),
        table = arm.table(),
    )
}

/// `transactions.id` → position, by a seek on the `transactions` key.
pub(crate) fn positions_by_id_sql(keys: &[(i64, i64)]) -> String {
    let mut ledgers: Vec<i64> = keys.iter().map(|k| k.0).collect();
    ledgers.sort_unstable();
    ledgers.dedup();
    let ledgers = ledgers
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let pairs = keys
        .iter()
        .map(|(l, id)| format!("({l},{id})"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "SELECT ledger_sequence, application_order FROM transactions \
         WHERE ledger_sequence IN ({ledgers}) AND (ledger_sequence, id) IN ({pairs}) \
         LIMIT 1 BY ledger_sequence, application_order"
    )
}

#[derive(Debug, Row, Deserialize)]
struct PositionRow {
    ledger_sequence: i64,
    application_order: i16,
}

#[derive(Debug, Row, Deserialize)]
struct IdRow {
    ledger_sequence: i64,
    transaction_id: i64,
}

#[allow(clippy::too_many_arguments)]
async fn arm_window(
    client: &clickhouse::Client,
    arm: Arm,
    contract_id: i64,
    partition: &str,
    head_max: &str,
    cursor: Option<Position>,
    direction: Direction,
    window: usize,
) -> Result<ArmWindow, clickhouse::error::Error> {
    let cursor_ledger = cursor.map(|c| c.0);
    let last_ledger = client
        .query(&window_last_ledger_sql(
            arm,
            contract_id,
            partition,
            head_max,
            cursor_ledger,
            direction,
            window,
        ))
        .fetch_optional::<i64>()
        .await?;
    let rows_sql = window_rows_sql(
        arm,
        contract_id,
        partition,
        head_max,
        cursor_ledger,
        last_ledger,
        direction,
    );
    let positions: Vec<Position> = match arm {
        Arm::Events => client
            .query(&rows_sql)
            .fetch_all::<PositionRow>()
            .await?
            .into_iter()
            .map(|r| (r.ledger_sequence, r.application_order))
            .collect(),
        Arm::Invocations | Arm::Operations => {
            let keys: Vec<(i64, i64)> = client
                .query(&rows_sql)
                .fetch_all::<IdRow>()
                .await?
                .into_iter()
                .map(|r| (r.ledger_sequence, r.transaction_id))
                .collect();
            if keys.is_empty() {
                Vec::new()
            } else {
                client
                    .query(&positions_by_id_sql(&keys))
                    .fetch_all::<PositionRow>()
                    .await?
                    .into_iter()
                    .map(|r| (r.ledger_sequence, r.application_order))
                    .collect()
            }
        }
    };
    let desc = descending(direction);
    let positions = positions
        .into_iter()
        .filter(|&p| match cursor {
            Some(c) if desc => p < c,
            Some(c) => p > c,
            None => true,
        })
        .collect();
    Ok(ArmWindow {
        positions,
        truncated: last_ledger.is_some(),
        last_ledger,
    })
}

/// Up to `take` positions of transactions touching `contract_id`, past
/// `cursor`, in page order. `partition` is a predicate on `ledger_sequence`
/// (the list pages one partition at a time); `head_max` caps the ledger.
pub async fn contract_tx_positions(
    client: &clickhouse::Client,
    contract_id: i64,
    partition: &str,
    head_max: &str,
    cursor: Option<Position>,
    direction: Direction,
    take: usize,
) -> Result<Vec<Position>, clickhouse::error::Error> {
    let mut window = take.max(1);
    let mut round = 1;
    loop {
        let window_of = |arm| {
            arm_window(
                client,
                arm,
                contract_id,
                partition,
                head_max,
                cursor,
                direction,
                window,
            )
        };
        let (events, invocations, operations) = tokio::try_join!(
            window_of(Arm::Events),
            window_of(Arm::Invocations),
            window_of(Arm::Operations),
        )?;
        match merge_arm_windows(&[events, invocations, operations], direction, take) {
            MergeResult::Page(page) => return Ok(page),
            MergeResult::NeedWiderWindow(partial) if round == MAX_ROUNDS => {
                tracing::warn!(
                    contract_id,
                    window,
                    returned = partial.len(),
                    "contract transaction list: window cap reached, short page"
                );
                return Ok(partial);
            }
            MergeResult::NeedWiderWindow(_) => {
                window *= 2;
                round += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests;
