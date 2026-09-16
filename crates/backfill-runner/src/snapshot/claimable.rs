//! Claimable balances in the checkpoint seed (task 0210, ADR 0057 decision 5).
//!
//! `claimable_balance_holdings` is built from ledger changes, so it is right
//! only if every ledger since its writer shipped reached that writer. This is
//! the half that makes it right anyway: balances created before our history
//! (and never touched since) are inserted from the snapshot, and a balance our
//! side still shows live after the network removed it is closed.
//!
//! The comparison is the balances one — same [`verdict::verdict`], same
//! [`verdict::correction`], same versioning contract — with two differences:
//!
//! - **Keyed by balance alone.** A dead record carries only the balance id, so
//!   the network side cannot key on the asset. A live network balance whose
//!   asset differs from our row's is left unmatched: our row closes as gone and
//!   the network's is inserted under its real asset.
//! - **The writer must predate the checkpoint.** A balance claimed between the
//!   checkpoint and the writer deploy is live in the snapshot and has no
//!   tombstone on our side, so seeding from that checkpoint writes it live for
//!   good. See [`writer_coverage`].

use std::collections::HashSet;
use std::fmt::Write as _;

use db_clickhouse::persist::rows::BalanceRow;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::network_state::{NetHolding, NetworkState};
use crate::snapshot::report::Report;
use crate::snapshot::slices::{key_slices, slice_sql};
use crate::snapshot::verdict::{self, OurRow, Verdict};

pub(crate) const TABLE: &str = "claimable_balance_holdings";

#[derive(Default)]
pub(crate) struct ClaimableCorrections {
    pub(crate) rows: Vec<BalanceRow>,
    /// Our positive rows this run zeroes, like `ghosts.tsv` for `balances`.
    pub(crate) ghosts: Vec<String>,
}

/// Whether a seed from `checkpoint` can trust our table.
///
/// Claimable balances are removed in practically every ledger (655,838
/// removals in 100,000 ledgers, 2026-09-15), and the writer tombstones every
/// removal it sees. So the first tombstone in the table marks when the writer
/// started, to within a ledger or two, and every removal after it is recorded.
/// A checkpoint at or after that tombstone is covered; an earlier one is not.
/// No tombstone at all means the writer has not run.
///
/// Seed-written closures also carry `closed_at_ledger`, but only from a run
/// that already passed this check, so they never move the minimum earlier.
///
/// The minimum says when the writer started, not that it never stopped, and a
/// backfill writes this table for whatever range it is given. Both hold because
/// of how the range is chosen: a `--reindex` covers the whole Soroban era up to
/// the tip, a gap-fill ends where the live writer resumed, and a seed runs only
/// after the backfill has finished. A re-parse of a bounded OLD range would
/// break the check — it would leave tombstones below the deploy and balances
/// claimed after its end live with no tombstone. The rule is in
/// `docs/backfills.md` ("Never re-parse a range that ends before…").
pub(crate) fn writer_coverage(first_tombstone: Option<i64>, checkpoint: u32) -> Result<(), String> {
    match first_tombstone {
        None => Err(format!(
            "{TABLE} holds no closed balance — the claimable balance writer has not run, \
             so a claim after checkpoint {checkpoint} would be seeded as live for good"
        )),
        Some(first) if first > i64::from(checkpoint) => Err(format!(
            "checkpoint {checkpoint} predates the writer's first recorded claim at ledger \
             {first} — balances claimed in between would be seeded as live for good; \
             wait for a later checkpoint"
        )),
        Some(_) => Ok(()),
    }
}

pub(crate) async fn first_writer_tombstone(sink: &Sink) -> Result<Option<i64>, BackfillError> {
    // `min` over an empty non-Nullable column returns 0, not NULL, so the
    // count decides whether there is a minimum at all.
    let (closed, first): (u64, i64) = sink
        .client()
        .query(&format!(
            "SELECT count(), min(closed_at_ledger) FROM {TABLE} WHERE closed_at_ledger > 0"
        ))
        .fetch_one()
        .await?;
    Ok((closed > 0).then_some(first))
}

/// Claim the network balance for one of our rows — the `verdict::claim` of
/// this table.
pub(crate) fn claim(state: &mut NetworkState, row: &OurRow) -> Option<NetHolding> {
    let (entry, network_asset) = state.claimable_balances.get_mut(&row.holder_id)?;
    if entry.live && *network_asset != Some(row.asset_id) {
        return None;
    }
    entry.matched = true;
    Some(*entry)
}

/// Compare our table with the snapshot and build its corrections. Credit
/// assets the inserted rows point at are added to `referenced_assets`, so the
/// seed's stub pass gives them an `assets` row when we have none.
///
/// No floor on our side: a short read only turns our rows into "missing"
/// network balances, which are inserted at their own ledger and lose to any
/// newer row. The dangerous short read is the snapshot's, and
/// `open_snapshot` floors that.
pub(crate) async fn build_corrections(
    sink: &Sink,
    state: &mut NetworkState,
    checkpoint: u32,
    report: &mut Report,
    referenced_assets: &mut HashSet<i64>,
) -> Result<ClaimableCorrections, BackfillError> {
    let mut out = ClaimableCorrections::default();

    let mut read = 0u64;
    for (from, to) in key_slices() {
        let mut cursor = sink
            .client()
            .query(&slice_sql(TABLE, "", from, to))
            .fetch::<OurRow>()?;
        while let Some(row) = cursor.next().await? {
            read += 1;
            let net = claim(state, &row);
            let v = verdict::verdict(&row, net.as_ref(), checkpoint);
            report.claimable.observe(v, row.amount);
            if v == Verdict::Ghost {
                out.ghosts.push(format!(
                    "{}\t{}\t{}\t{}",
                    row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
                ));
            }
            if let Some(c) = verdict::correction(v, net.as_ref(), checkpoint) {
                out.rows.push(BalanceRow {
                    holder_id: row.holder_id,
                    asset_id: row.asset_id,
                    amount: c.amount,
                    last_updated_ledger: c.last_updated_ledger,
                    closed_at_ledger: c.closed_at_ledger,
                });
            }
        }
    }
    println!("  folded {read} of our claimable balance rows");

    for (holder_id, (e, asset_id)) in &state.claimable_balances {
        let (true, false, Some(asset_id)) = (e.live, e.matched, *asset_id) else {
            continue;
        };
        report.claimable.missing += 1;
        out.rows.push(BalanceRow {
            holder_id: *holder_id,
            asset_id,
            amount: i128::from(e.balance),
            last_updated_ledger: i64::from(e.ledger),
            closed_at_ledger: 0,
        });
        referenced_assets.insert(asset_id);
    }
    Ok(out)
}

/// The claimable balance block of `summary.txt`.
pub(crate) fn render_summary(report: &Report, coverage: &Result<(), String>) -> String {
    let mut out = report
        .claimable
        .render("CLAIMABLE BALANCES (ClaimableBalanceEntry)", false);
    match coverage {
        Ok(()) => out.push_str("    writer coverage: checkpoint is covered\n"),
        Err(why) => {
            let _ = writeln!(out, "    !! writer coverage: {why} (--execute refuses)");
        }
    }
    out
}

#[cfg(test)]
#[path = "claimable_tests.rs"]
mod tests;
