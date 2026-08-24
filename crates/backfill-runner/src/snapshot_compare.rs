//! Four-way comparison of our `balances` against the checkpoint snapshot
//! (task 0463 step 3c, option A — in process, no staging table, no writes).
//!
//! ## Why four numbers and never one
//!
//! A single "match %" hides the direction of every error. The four are:
//!
//! | bucket        | meaning                                          |
//! |---------------|--------------------------------------------------|
//! | **missing**   | the network holds it, we have no row at all      |
//! | **closure**   | we hold it at zero, the network says it is gone  |
//! | **anomaly**   | we hold it at a POSITIVE amount, network says gone |
//! | **divergent** | both hold it, the amounts disagree               |
//! | **stale**     | both hold it, our `last_updated_ledger` is behind |
//!
//! **`anomaly` is the one that must never be silently folded into `closure`.**
//! A positive-but-absent row means an ingestion gap, and writing it off as a
//! closure would let the read flip render it as a live holding carrying a
//! number that is simply false — worse than the bug being fixed.
//!
//! ## Why the read is chunked by key range
//!
//! One `GROUP BY holder_id, asset_id` over the whole table dies:
//! `MEMORY_LIMIT_EXCEEDED … maximum: 3.73 GiB` at ~48.6M groups (measured
//! 2026-08-18). `holder_id` is the leading `ORDER BY` column, so slicing on it
//! turns each chunk into a primary-key range scan with a few hundred thousand
//! groups — bounded server memory, no reliance on global result ordering.
//!
//! ## What is deliberately out of scope, and reported as such
//!
//! - **Contract-held classic balances** (`holder_id` in `soroban_contracts`) —
//!   a contract holds a classic asset through the SAC's `ContractData`, not a
//!   trustline, so the snapshot's trustline set would call every one of them a
//!   phantom. Excluded and COUNTED.
//! - **Type-3 Soroban holdings** — same reason, different entry type.
//! - **Pool shares** — same ledger entry type as a trustline, but our side
//!   keeps them in `lp_positions`. Tallied from the snapshot, not compared.

use std::path::Path;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::{self};
use crate::snapshot_report::Report;

/// Number of `holder_id` slices. 64 keeps each chunk near the ~760k groups
/// measured for 1/64 of the key space — two orders under the server's limit.
const KEY_SLICES: i128 = 64;

/// Stream our deduplicated `balances` in `holder_id` slices, invoking `f` per
/// row. The ONE read path both the comparison and the seed use — like every
/// other corrective command in this crate, the tool reads its own inputs
/// through `sink.client()`; there is no manual export step. (A hand-exported
/// TSV transport existed during the research phase and was removed in the
/// 2026-08-20 review: the binary holds the same connection for `--execute`
/// inserts anyway, and a cursor error propagates loudly where the operator
/// CLI's exit-0-on-server-error trap did not.)
///
/// Errors below [`snapshot::MIN_OUR_ROWS`] rows: a short read (wrong database,
/// dropped slice) would silently report our own holdings as a phantom network
/// gap.
pub(crate) async fn stream_our_rows(
    sink: &Sink,
    mut f: impl FnMut(&snapshot::OurRow),
) -> Result<u64, BackfillError> {
    let mut seen = 0u64;
    for (i, (from, to)) in key_slices().enumerate() {
        // `argMax` collapses the ReplacingMergeTree duplicates the way a read
        // must: prod tables carry unmerged parts, so a plain SELECT double-counts.
        let mut cursor = sink
            .client()
            .query(&slice_sql(from, to))
            .fetch::<snapshot::OurRow>()?;
        while let Some(row) = cursor.next().await? {
            seen += 1;
            f(&row);
        }
        println!("    slice {:>2}/{KEY_SLICES} — {seen} rows so far", i + 1);
    }
    if seen < snapshot::MIN_OUR_ROWS {
        return Err(BackfillError::Incomplete(format!(
            "our balances read returned {seen} rows, expected at least {} — a short \
             read reports our own holdings as a phantom network gap (wrong database?)",
            snapshot::MIN_OUR_ROWS
        )));
    }
    Ok(seen)
}

/// The per-slice read. Its SELECT list IS the field order of
/// [`snapshot::OurRow`] — a mismatch would silently misclassify every row
/// rather than fail.
fn slice_sql(from: i128, to: i128) -> String {
    // The aggregates are aliased INSIDE a subquery and renamed outside. Aliasing
    // `max(last_updated_ledger) AS last_updated_ledger` directly shadows the
    // column, so the next `argMax(..., last_updated_ledger)` binds the alias and
    // ClickHouse rejects it: "Aggregate function max(...) is found inside
    // another aggregate function" (ILLEGAL_AGGREGATION).
    format!(
        "SELECT holder_id, asset_id, amt AS amount, led AS last_updated_ledger, \
                cls AS closed_at_ledger \
         FROM ( \
             SELECT holder_id, \
                    asset_id, \
                    argMax(amount, last_updated_ledger) AS amt, \
                    max(last_updated_ledger) AS led, \
                    argMax(closed_at_ledger, last_updated_ledger) AS cls \
             FROM balances \
             WHERE holder_id BETWEEN {from} AND {to} \
               AND asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
               AND holder_id NOT IN (SELECT id FROM soroban_contracts) \
             GROUP BY holder_id, asset_id \
         )"
    )
}

/// The slice boundaries, covering the i64 key space exactly once.
fn key_slices() -> impl Iterator<Item = (i128, i128)> {
    let lo = i128::from(i64::MIN);
    let hi = i128::from(i64::MAX);
    let step = (hi - lo + 1) / KEY_SLICES;
    (0..KEY_SLICES).map(move |s| {
        let from = lo + s * step;
        let to = if s == KEY_SLICES - 1 {
            hi
        } else {
            lo + (s + 1) * step - 1
        };
        (from, to)
    })
}

/// Read-only four-way comparison. Downloads and decodes the snapshot, streams
/// our rows, prints the counts. **Writes nothing, anywhere.**
pub async fn compare_command(
    sink: &Sink,
    dump_dir: Option<&Path>,
    pinned_manifest: Option<&Path>,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    // Details (StrKeys, asset identities) cost ~2 GB extra and exist so the
    // sample dumps can carry REAL keys next to the surrogates. Without them
    // verifying a "missing" entry meant reversing a one-way hash through our
    // own incomplete tables — the very tables under audit. Only paid when
    // dumps were asked for.
    let snapshot::SnapshotPass { list, mut state } =
        snapshot::open_snapshot(pinned_manifest, dump_dir.is_some(), "").await?;

    let mut report = Report::new(list.checkpoint_ledger);
    println!("\n  streaming our balances in {KEY_SLICES} key slices…");
    stream_our_rows(sink, |row| {
        report.observe(row, &mut state);
    })
    .await?;
    for (key, entry) in &state.trustlines {
        if entry.live && !entry.matched {
            report.observe_missing_trustline(key, entry, &state);
        }
    }
    for entry in state.accounts.values() {
        if entry.live && !entry.matched {
            report.observe_missing_account();
        }
    }

    print!("{}", report.render_missing_histogram());
    if let Some(dir) = dump_dir {
        report.write_dumps(dir)?;
    }

    report.classic.print("CLASSIC CREDIT trustlines", false);
    report
        .native
        .print("NATIVE XLM holdings (AccountEntry, not a trustline)", true);

    // Excluded on purpose — printed so the pass never reads as exhaustive when
    // it is not.
    {
        let excluded_contract: u64 = sink
            .client()
            .query(
                "SELECT count() FROM balances \
             WHERE asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
               AND holder_id IN (SELECT id FROM soroban_contracts)",
            )
            .fetch_one()
            .await?;
        let excluded_type3: u64 = sink
        .client()
        .query("SELECT count() FROM balances WHERE asset_id IN (SELECT id FROM assets WHERE asset_type = 3)")
        .fetch_one()
        .await?;

        println!("\n  NOT COMPARED (deliberate, see module docs)");
        println!("    contract-held classic rows  {excluded_contract:>12}");
        println!("    type-3 Soroban rows         {excluded_type3:>12}");
    }
    println!(
        "    snapshot pool shares        {:>12}  (our side: lp_positions)",
        state.live_pool_shares()
    );
    println!(
        "\n  total {:.1}s — nothing was written",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
