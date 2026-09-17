//! `balances` in the checkpoint seed (task 0463): our classic and native
//! holdings against the snapshot's trustlines and accounts. Moved out of
//! `seed.rs` so every compared table has its own module next to
//! [`crate::snapshot::claimable`] and [`crate::snapshot::pools`].

use std::collections::HashSet;

use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::BalanceRow;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::network_state::{self, NetworkState};
use crate::snapshot::report::Report;
use crate::snapshot::slices::{KEY_SLICES, key_slices, slice_sql};
use crate::snapshot::verdict;

#[derive(Default)]
pub(crate) struct BalanceCorrections {
    pub(crate) rows: Vec<BalanceRow>,
    /// One line per row this run zeroes while it still held a positive amount
    /// — the anomaly report, and the only pre-image of what `--execute` takes
    /// away.
    pub(crate) ghosts: Vec<String>,
}

/// Stream our deduplicated `balances` in `holder_id` slices, invoking `f` per
/// row. Like every
/// other corrective command in this crate, the tool reads its own inputs
/// through `sink.client()`; there is no manual export step. (A hand-exported
/// TSV transport existed during the research phase and was removed with the
/// 2026-08-21 self-read decision: the binary holds the same connection for `--execute`
/// inserts anyway, and a cursor error propagates loudly where the operator
/// CLI's exit-0-on-server-error trap did not.)
///
/// A short read cannot pass silently: a failed slice errors the cursor, and
/// `seed::refuse_if_reads_can_truncate` rules out a profile that would cut a
/// result instead of failing it.
async fn stream_our_rows(
    sink: &Sink,
    mut f: impl FnMut(&verdict::OurRow),
) -> Result<u64, BackfillError> {
    let mut seen = 0u64;
    for (i, (from, to)) in key_slices().enumerate() {
        // `argMax` collapses the ReplacingMergeTree duplicates the way a read
        // must: prod tables carry unmerged parts, so a plain SELECT double-counts.
        let mut cursor = sink
            .client()
            .query(&slice_sql("balances", BALANCES_FILTER, from, to))
            .fetch::<verdict::OurRow>()?;
        while let Some(row) = cursor.next().await? {
            seen += 1;
            f(&row);
        }
        println!("    slice {:>2}/{KEY_SLICES} — {seen} rows so far", i + 1);
    }
    Ok(seen)
}

/// The `balances` rows the snapshot models: classic and native assets, not held
/// by a contract.
const BALANCES_FILTER: &str = "AND asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
                               AND holder_id NOT IN (SELECT id FROM soroban_contracts)";

/// Emit the correction one verdict implies. The verdict comes from the REPORT,
/// which counted and sampled the same row a moment earlier — so the summary an
/// operator signs off on and the rows `--execute` writes are derived from one
/// classification, not two.
///
/// Within ONE run. Across two runs the populations differ, and the honest
/// statement of how is worth spelling out, because an earlier version of this
/// comment claimed more than it could:
///
/// - OUR side drifts harmlessly. `--execute` re-reads our rows fresh, like
///   every corrective command here, and anything the live writer touched since
///   is absorbed by the `>= checkpoint` guard — those rows are newly LEFT
///   ALONE, never given a different correction.
/// - The SNAPSHOT side drifts too, and that half the old comment did not
///   reason about. Checkpoints publish every 64 ledgers (~5 minutes) while a
///   full pass takes ~5 (measured: 317 s dry-run, 637 s with the inserts;
///   the archive download dominates and is network-bound, so earlier runs
///   measured 909 s on the same code). What separates two runs is therefore a
///   whole pass PLUS the operator reading `summary.txt`, which is why
///   `--execute` still always decodes a later checkpoint than the dry-run
///   reviewed — but the margin is one checkpoint interval, not three.
///   Holdings the network
///   created in that window are `missing` in the second run and get INSERTED,
///   without having appeared in the summary an operator signed off on.
///
/// That drift is accepted deliberately (2026-08-21, reaffirmed 2026-08-26).
/// The rows it adds are real live holdings — the fresher snapshot is the
/// better input, not a riskier one — and the run is verified by measuring its
/// OUTCOME against the network (coverage, the 200-account chain probe,
/// aggregate deltas), which a frozen input would not improve. `manifest.json`
/// records the checkpoint each run actually used, so the population is always
/// identifiable after the fact.
fn fold_our_row(
    row: &verdict::OurRow,
    verdict: verdict::Verdict,
    net: Option<network_state::NetHolding>,
    checkpoint: u32,
    out: &mut BalanceCorrections,
) {
    use verdict::Verdict as V;
    if verdict == V::Ghost {
        out.ghosts.push(format!(
            "{}\t{}\t{}\t{}",
            row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
        ));
    }
    let Some(c) = verdict::correction(verdict, net.as_ref(), checkpoint) else {
        return;
    };
    out.rows.push(BalanceRow {
        holder_id: row.holder_id,
        asset_id: row.asset_id,
        amount: c.amount,
        last_updated_ledger: c.last_updated_ledger,
        closed_at_ledger: c.closed_at_ledger,
    });
}

/// Pass 1 (our rows, classified by the report) and pass 2 (live snapshot
/// entries nobody matched). Assets and holders the inserted rows point at are
/// added to the two sets, so the seed's stub pass can define them.
pub(crate) async fn build_corrections(
    sink: &Sink,
    state: &mut NetworkState,
    checkpoint: u32,
    report: &mut Report,
    referenced_assets: &mut HashSet<i64>,
    referenced_holders: &mut HashSet<i64>,
) -> Result<BalanceCorrections, BackfillError> {
    let mut out = BalanceCorrections::default();

    // Pass 1: our rows → the report classifies, counts and samples; the verdict
    // it hands back drives the correction. One classification, two outputs.
    println!("\n  streaming our balances in {KEY_SLICES} key slices…");
    let rows_read = stream_our_rows(sink, |row| {
        let (v, net) = report.observe(row, state);
        fold_our_row(row, v, net, checkpoint, &mut out);
    })
    .await?;
    println!("  folded {rows_read} of our rows");

    // Pass 2: unmatched live snapshot entries → missing-holding inserts.
    for (key, e) in &state.trustlines {
        if e.live && !e.matched {
            report.observe_missing_trustline(key, e, state);
            out.rows.push(BalanceRow {
                holder_id: key.holder_id,
                asset_id: key.asset_id,
                amount: i128::from(e.balance),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
            referenced_assets.insert(key.asset_id);
            referenced_holders.insert(key.holder_id);
        }
    }
    for (id, e) in &state.accounts {
        if e.live && !e.matched {
            report.observe_missing_account(*id, e, state);
            out.rows.push(BalanceRow {
                holder_id: *id,
                asset_id: ids::NATIVE_ASSET_ID,
                amount: i128::from(e.balance),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
            referenced_holders.insert(*id);
        }
    }
    Ok(out)
}

/// Rows of `balances` the comparison leaves out, counted for the summary.
pub(crate) async fn excluded_counts(sink: &Sink) -> Result<(u64, u64), BackfillError> {
    // Excluded on purpose — reported so the pass never reads as exhaustive
    // when it is not. Contract-held classic balances live in the SAC's
    // `ContractData`, not a trustline, so the snapshot's trustline set would
    // call every one of them a phantom; type-3 is the same reason, different
    // entry type; pool shares are the same ledger entry type but live in
    // `lp_positions` on our side (ADR 0056 merges them).
    // `uniqExact`, not `count()`: production tables carry unmerged
    // ReplacingMergeTree parts, so a raw row count is 2-3x the number of
    // holdings (measured 182,370 rows over 70,347 keys). The compared
    // population is counted per KEY — see `slice_sql`'s GROUP BY — and a
    // report that mixes the two bases invites exactly the comparison its
    // reader will make.
    let contract: u64 = sink
        .client()
        .query(
            "SELECT uniqExact((holder_id, asset_id)) FROM balances \
             WHERE asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
               AND holder_id IN (SELECT id FROM soroban_contracts)",
        )
        .fetch_one()
        .await?;
    let type3: u64 = sink
        .client()
        .query(
            "SELECT uniqExact((holder_id, asset_id)) FROM balances \
             WHERE asset_id IN (SELECT id FROM assets WHERE asset_type = 3)",
        )
        .fetch_one()
        .await?;

    Ok((contract, type3))
}
