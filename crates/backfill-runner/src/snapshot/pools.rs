//! Classic liquidity pools in the checkpoint seed (task 0210).
//!
//! `balance_aggregates_mv` takes a classic pool's reserves from its newest
//! `liquidity_pool_snapshots` row. A pool unchanged since the ingest floor has no
//! row at all — no ledger we processed touched it — so its reserves are missing
//! from supply, and no re-parse can add it. The checkpoint holds every live pool
//! entry, so this pass inserts what it knows and we do not: a snapshot, and the
//! pool's dimension row, for every live pool whose entry is newer than our newest
//! snapshot of it.
//!
//! Insert-only. Rows are built by the live builders
//! (`db_clickhouse::persist::classic_pools`) from the entry read as an `updated`
//! change (its value as of its own last modification), and versioned on the entry's own `lastModifiedLedgerSeq`, so any
//! newer live row wins and a re-run inserts the same rows again. There is no
//! writer-coverage check: the pool writer predates every checkpoint we can read.
//! A pool the network removed while our newest snapshot still shows reserves is
//! reported, not corrected — a removal carries no reserves to write.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use db_clickhouse::persist::classic_pools;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{LiquidityPoolRow, LiquidityPoolSnapshotRow};
use stellar_xdr::{LedgerEntry, LedgerEntryData, LiquidityPoolEntryBody};

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::network_state::{NetworkState, classic_asset};

#[derive(Default)]
pub(crate) struct PoolCorrections {
    pub(crate) pool_rows: Vec<LiquidityPoolRow>,
    pub(crate) snapshot_rows: Vec<LiquidityPoolSnapshotRow>,
    /// Live pools with no snapshot of ours.
    pub(crate) missing: u64,
    /// Live pools whose entry changed after our newest snapshot.
    pub(crate) stale: u64,
    /// `pool id hex \t our newest snapshot ledger` for pools gone from the
    /// network while our newest snapshot still holds reserves.
    pub(crate) gone_with_reserves: Vec<String>,
}

/// Our newest snapshot of one pool.
#[derive(clickhouse::Row, serde::Deserialize)]
struct OurNewest {
    pool_id: [u8; 32],
    ledger: i64,
    has_reserves: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Need {
    Missing,
    Stale,
    Current,
}

/// Whether the network's entry, last modified at `entry_ledger`, adds anything
/// to our newest snapshot of the same pool.
pub(crate) fn need(our_newest_ledger: Option<i64>, entry_ledger: u32) -> Need {
    match our_newest_ledger {
        None => Need::Missing,
        Some(ours) if ours < i64::from(entry_ledger) => Need::Stale,
        Some(_) => Need::Current,
    }
}

/// Our pool and snapshot rows for one live pool entry, built exactly as live
/// ingest builds them.
pub(crate) fn rows_for_entry(
    entry: &LedgerEntry,
) -> Result<(Vec<LiquidityPoolRow>, Vec<LiquidityPoolSnapshotRow>), BackfillError> {
    let change = xdr_parser::ledger_entry_changes::entry_as_current_change(entry)
        .ok_or_else(|| BackfillError::Incomplete("pool entry did not decode".into()))?;
    let (pools, snapshots) = xdr_parser::state::extract_liquidity_pools(&[change]);
    let staging = |e: db_clickhouse::SchemaError| BackfillError::Incomplete(e.to_string());
    Ok((
        classic_pools::build_pool_rows(&pools).map_err(staging)?,
        classic_pools::build_snapshot_rows(&snapshots, &HashMap::new()).map_err(staging)?,
    ))
}

/// Whether our pool belongs in `pools_gone.tsv`: it still holds reserves, the
/// snapshot has no live entry for it, and our newest row predates the
/// checkpoint — a pool created after the checkpoint is simply not in it yet.
pub(crate) fn gone_with_reserves(
    has_reserves: bool,
    live: bool,
    our_ledger: i64,
    checkpoint: u32,
) -> bool {
    has_reserves && !live && our_ledger < i64::from(checkpoint)
}

pub(crate) async fn build_corrections(
    sink: &Sink,
    state: &mut NetworkState,
    checkpoint: u32,
    referenced_assets: &mut HashSet<i64>,
) -> Result<PoolCorrections, BackfillError> {
    let mut ours: HashMap<[u8; 32], OurNewest> = HashMap::new();
    let mut cursor = sink
        .client()
        .query(
            "SELECT pool_id, max(ledger_sequence) AS ledger, \
                    toUInt8(argMax(reserve_a, ledger_sequence) > 0 \
                            OR argMax(reserve_b, ledger_sequence) > 0) AS has_reserves \
             FROM liquidity_pool_snapshots \
             WHERE pool_id IN (SELECT pool_id FROM liquidity_pools WHERE pool_kind = 0) \
             GROUP BY pool_id",
        )
        .fetch::<OurNewest>()?;
    while let Some(row) = cursor.next().await? {
        ours.insert(row.pool_id, row);
    }
    println!("  read our newest snapshot of {} pools", ours.len());

    let mut out = PoolCorrections::default();
    let mut new_assets: Vec<(i64, Option<(String, String)>)> = Vec::new();
    for (pool_id, entry) in &state.pools {
        let Some(entry) = entry else { continue };
        let kind = need(
            ours.get(pool_id).map(|o| o.ledger),
            entry.last_modified_ledger_seq,
        );
        match kind {
            Need::Current => continue,
            Need::Missing => out.missing += 1,
            Need::Stale => out.stale += 1,
        }
        let (pool_rows, snapshot_rows) = rows_for_entry(entry)?;
        out.pool_rows.extend(pool_rows);
        out.snapshot_rows.extend(snapshot_rows);
        if let LedgerEntryData::LiquidityPool(lp) = &entry.data {
            let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &lp.body;
            new_assets.push(classic_asset(&cp.params.asset_a));
            new_assets.push(classic_asset(&cp.params.asset_b));
        }
    }
    // Legs the stub pass must define, like any other seeded holding's asset.
    for (asset_id, identity) in new_assets {
        if asset_id == ids::NATIVE_ASSET_ID {
            continue;
        }
        if let Some(identity) = identity {
            state.asset_registry.entry(asset_id).or_insert(identity);
        }
        referenced_assets.insert(asset_id);
    }

    for (pool_id, o) in &ours {
        let live = matches!(state.pools.get(pool_id), Some(Some(_)));
        if gone_with_reserves(o.has_reserves == 1, live, o.ledger, checkpoint) {
            out.gone_with_reserves
                .push(format!("{}\t{}", hex::encode(pool_id), o.ledger));
        }
    }
    Ok(out)
}

/// The pool block of `summary.txt`.
pub(crate) fn render_summary(corr: &PoolCorrections) -> String {
    let mut out = String::from("\n  CLASSIC POOLS (LiquidityPoolEntry → newest snapshot)\n");
    let _ = writeln!(
        out,
        "    live pools with no snapshot of ours   {:>12}",
        corr.missing
    );
    let _ = writeln!(
        out,
        "    live pools newer than our snapshot    {:>12}",
        corr.stale
    );
    let _ = writeln!(
        out,
        "    gone from the network, reserves ours  {:>12}  (reported only: pools_gone.tsv)",
        corr.gone_with_reserves.len()
    );
    out
}

#[cfg(test)]
#[path = "pools_tests.rs"]
mod tests;
