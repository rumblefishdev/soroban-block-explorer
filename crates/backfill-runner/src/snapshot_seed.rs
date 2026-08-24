//! The checkpoint-snapshot seed (task 0463 step 3d, ADR 0055) — build every
//! correction the four-way comparison proved necessary, as INSERT-ready rows.
//!
//! ## What it writes (with `--execute`; without it, artifacts + counts only)
//!
//! | correction | rows measured 2026-08-18 | version ledger | closed_at |
//! |---|---|---|---|
//! | missing live holding | ~19.3M classic | the ENTRY's own `lastModifiedLedgerSeq` | 0 |
//! | closure (ours 0, gone) | ~22.2M classic + 2.3M native | checkpoint | checkpoint |
//! | ghost (ours >0, gone) | ~1.04M native + ~2k classic | checkpoint | checkpoint |
//! | self-heal (snapshot newer) | ~25k | the entry's own ledger | 0 |
//! | `account_entry_state` full seed | every live account | the entry's own ledger | — |
//! | `assets` / `accounts` dimension stubs | the referenced ids we lack | entry ledger | — |
//!
//! ## The versioning contract (the load-bearing part)
//!
//! - **Live data always versions on the entry's own `lastModifiedLedgerSeq`**,
//!   never a window boundary — the task 0492 defect. The live parser's newer
//!   writes then win regardless of load order.
//! - **Closures version on the checkpoint ledger.** There is no entry left to
//!   carry a ledger, and the checkpoint IS the ledger of the observation
//!   "this is gone". `closed_at_ledger` gets the same value and therefore
//!   means "closed AT OR BEFORE this ledger" for seeded rows (the writer's own
//!   stamps are exact). Side effect: every seed-stamped closure shares the run's
//!   checkpoint value — a recognisable cohort, which is the only provenance we
//!   have until task 0492 lands a real convention.
//! - **Ghosts (positive-but-absent) are corrected AND reported, never silent.**
//!   The standing policy said "anomaly report, not silent closure" back when
//!   these were presumed ingestion gaps; RPC verification (100/100 sampled
//!   accounts ABSENT on chain, amounts byte-checked) proved them real
//!   removals, so option A applies — `amount = 0` together with
//!   `closed_at_ledger` — but the full list still lands in the artifacts dir.
//!
//! ## Deployment order — DO NOT reorder
//!
//! 1. Deploy the lifecycle writer (stage.rs stamps closures itself).
//! 2. THEN run this seed against a checkpoint taken AFTER that deploy.
//!
//! Reversed, every removal between the checkpoint and the deploy is written by
//! the OLD writer as a plain `amount = 0, closed_at = 0` row with a ledger
//! ABOVE the checkpoint — it outversions the seed's closure and resurrects the
//! ghost. The 12-minute window of task 0310 taught the same lesson.
//!
//! ## What is deliberately NOT seeded here
//!
//! - **Pool-share trustlines** (77,048 live) — they live in `lp_positions`
//!   until the ADR 0056 merge lands. The archive is content-addressed (bucket
//!   files keyed by hash), so the LP pass re-derives this snapshot from the
//!   manifest JSON saved into the artifacts dir; nothing else must be kept.
//! - **Type-3 / contract-held holdings** — different ledger entry type
//!   (`ContractData`); their audit is task 0503 on the same decoder.
//!
//! ## Why the `assets` stubs need no RMT version
//!
//! `assets` is `ReplacingMergeTree` with NO version column, keyed on the
//! identity 4-tuple — on merge ClickHouse keeps the last-inserted row per
//! key. That is safe here because every field of an `AssetRow` (including the
//! `id` surrogate) is a pure function of that same identity tuple: any two
//! rows ever written for one key are byte-identical, so it cannot matter
//! which survives. Stubs are additionally emitted only for ids absent from
//! the known-id set read from ClickHouse, so they never contend with an existing row.

use std::collections::HashSet;
use std::path::Path;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::{self, PUBNET_ARCHIVE, SnapshotState};
use crate::snapshot_report::Report;
use crate::util::insert_rows;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{AccountEntryStateRow, AccountRow, AssetRow, BalanceRow};

/// Insert batch size. RowBinary streams; this only bounds peak buffering.
const INSERT_CHUNK: usize = 500_000;

/// All correction rows, still in memory, ready to insert or count.
///
/// **Deliberately materialised rather than streamed.** A dry-run over the full
/// network measured 4.5 GB peak RSS for ~44.9M balance rows plus 10.9M signer
/// rows — comfortable on the operator box, and holding them lets the run
/// report exact counts and write `summary.txt` BEFORE any insert, which is the
/// property that makes `--execute` reviewable. Streaming inserts per batch
/// would halve the memory but would start writing before the totals are known.
/// If this ever needs to run somewhere smaller, stream passes 1/2/4 and keep
/// only the counters — the insert ORDER (dimension stubs before the balances
/// that reference them) must be preserved.
#[derive(Default)]
struct Corrections {
    balances: Vec<BalanceRow>,
    entry_states: Vec<AccountEntryStateRow>,
    asset_stubs: Vec<AssetRow>,
    account_stubs: Vec<AccountRow>,
}

/// Number of `holder_id` slices. 64 keeps each chunk near the ~760k groups
/// measured for 1/64 of the key space — two orders under the server's limit.
const KEY_SLICES: i128 = 64;

/// Stream our deduplicated `balances` in `holder_id` slices, invoking `f` per
/// row. Like every
/// other corrective command in this crate, the tool reads its own inputs
/// through `sink.client()`; there is no manual export step. (A hand-exported
/// TSV transport existed during the research phase and was removed with the
/// 2026-08-21 self-read decision: the binary holds the same connection for `--execute`
/// inserts anyway, and a cursor error propagates loudly where the operator
/// CLI's exit-0-on-server-error trap did not.)
///
/// Errors below [`snapshot::MIN_OUR_ROWS`] rows: a short read (wrong database,
/// dropped slice) would silently report our own holdings as a phantom network
/// gap.
async fn stream_our_rows(
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

/// Fetch the set of existing dimension ids straight from ClickHouse — the
/// tool reads its own inputs, like every other corrective command here.
async fn fetch_id_set(sink: &Sink, sql: &str) -> Result<HashSet<i64>, BackfillError> {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct IdRow {
        id: i64,
    }
    let mut out = HashSet::new();
    let mut cursor = sink.client().query(sql).fetch::<IdRow>()?;
    while let Some(r) = cursor.next().await? {
        out.insert(r.id);
    }
    Ok(out)
}

/// Emit the correction one verdict implies. The verdict comes from the REPORT,
/// which counted and sampled the same row a moment earlier — so the summary an
/// operator signs off on and the rows `--execute` writes are derived from one
/// classification, not two. (`--execute` re-reads our rows fresh, like every
/// corrective command here; churn since the dry-run is absorbed by the
/// `>= checkpoint` guard, so the drift is only rows newly LEFT ALONE, never a
/// different correction.)
fn fold_our_row(
    row: &snapshot::OurRow,
    verdict: snapshot::Verdict,
    state: &mut SnapshotState,
    checkpoint: u32,
    out: &mut Corrections,
    ghost_log: &mut Vec<String>,
) {
    use snapshot::Verdict as V;
    let snapshot::OurRow {
        holder_id,
        asset_id,
        amount,
        last_updated_ledger,
        ..
    } = *row;
    match verdict {
        // Nothing to write: both sides agree (Stale = equal amounts, our ledger
        // merely older — the verdict rule guarantees the equality), our side is
        // the fresher one, or the snapshot is the stale side. Same-ledger
        // divergence is a defect signal: reported by the tally, never
        // auto-healed — picking a winner would bury the only evidence.
        V::AlreadyClosed
        | V::Agree
        | V::DivergentOursNewer
        | V::Stale
        | V::DivergentSameLedger
        | V::NewerThanCheckpoint => {}
        // We hid a holding the network says is live at a NEWER ledger: re-open
        // at the entry's own ledger, which outversions our wrong closure. Heal:
        // the snapshot is strictly newer AND the amounts differ, so adopt its
        // amount at ITS ledger.
        V::ClosedButLive | V::HealFromSnapshot => {
            let Some(e) = snapshot::snap_entry_for(state, row) else {
                return;
            };
            out.balances.push(BalanceRow {
                holder_id,
                asset_id,
                amount: i128::from(e.amount),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
        }
        V::Closure | V::Ghost => {
            if verdict == V::Ghost {
                ghost_log.push(format!(
                    "{holder_id}\t{asset_id}\t{amount}\t{last_updated_ledger}"
                ));
            }
            out.balances.push(BalanceRow {
                holder_id,
                asset_id,
                amount: 0,
                last_updated_ledger: i64::from(checkpoint),
                closed_at_ledger: i64::from(checkpoint),
            });
        }
    }
}

/// Build every correction. Deterministic function of (snapshot state, our
/// rows as read, dimension id sets) — a re-run against the same inputs
/// produces identical rows and RMT collapses them.
async fn build_corrections(
    sink: &Sink,
    state: &mut SnapshotState,
    known_assets: &HashSet<i64>,
    known_accounts: &HashSet<i64>,
    checkpoint: u32,
    report: &mut Report,
    ghost_log: &mut Vec<String>,
) -> Result<Corrections, BackfillError> {
    let mut out = Corrections::default();

    // Pass 1: our rows → the report classifies, counts and samples; the verdict
    // it hands back drives the correction. One classification, two outputs.
    println!("\n  streaming our balances in {KEY_SLICES} key slices…");
    let rows_read = stream_our_rows(sink, |row| {
        let v = report.observe(row, state);
        fold_our_row(row, v, state, checkpoint, &mut out, ghost_log);
    })
    .await?;
    println!("  folded {rows_read} of our rows");

    // Pass 2: unmatched live snapshot entries → missing-holding inserts.
    let mut referenced_assets: HashSet<i64> = HashSet::new();
    let mut referenced_holders: HashSet<i64> = HashSet::new();
    for (key, e) in &state.trustlines {
        if e.live && !e.matched {
            report.observe_missing_trustline(key, e, state);
            out.balances.push(BalanceRow {
                holder_id: key.holder_id,
                asset_id: key.asset_id,
                amount: i128::from(e.amount),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
            referenced_assets.insert(key.asset_id);
            referenced_holders.insert(key.holder_id);
        }
    }
    for (id, e) in &state.accounts {
        if e.live && !e.matched {
            report.observe_missing_account();
            out.balances.push(BalanceRow {
                holder_id: *id,
                asset_id: ids::NATIVE_ASSET_ID,
                amount: i128::from(e.amount),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
            referenced_holders.insert(*id);
        }
    }

    // Pass 3: dimension stubs — a seeded balance whose asset or holder has no
    // dimension row would render as a broken join, i.e. a new lie replacing an
    // old one. Issuers of stubbed assets count as referenced accounts too.
    for asset_id in &referenced_assets {
        if known_assets.contains(asset_id) {
            continue;
        }
        let Some((code, issuer)) = state.asset_registry.get(asset_id) else {
            continue;
        };
        let issuer_id = ids::account_id(issuer);
        out.asset_stubs.push(AssetRow {
            asset_type: 1,
            asset_code: code.clone(),
            issuer_id,
            contract_id: 0,
            id: *asset_id,
        });
        referenced_holders.insert(issuer_id);
    }
    for holder_id in &referenced_holders {
        if known_accounts.contains(holder_id) {
            continue;
        }
        let Some(d) = state.account_details.get(holder_id) else {
            continue; // referenced but not a live account in the snapshot
        };
        // `get`, not `[]`: safe today only because `absorb` fills `accounts`
        // and `account_details` together, which is an invariant rather than a
        // type guarantee — and this path runs after 4.5 GB of decode.
        let Some(entry) = state.accounts.get(holder_id).copied() else {
            continue;
        };
        out.account_stubs.push(AccountRow {
            id: *holder_id,
            account_id: d.strkey.clone(),
            // The entry's lastModified is an UPPER BOUND on creation; the true
            // first-seen predates our history. Better than a fabricated 0.
            first_seen_ledger: i64::from(entry.ledger),
            last_seen_ledger: i64::from(entry.ledger),
            sequence_number: d.seq_num,
            home_domain: (!d.home_domain.is_empty()).then(|| d.home_domain.clone()),
        });
    }

    // Pass 4: entry state — one row per live account (signers, thresholds,
    // flags), the FULL set, versioned on
    // the entry's own ledger so the (future) live writer wins on any change.
    for (id, e) in &state.accounts {
        if !e.live {
            continue;
        }
        let Some(d) = state.account_details.get(id) else {
            continue;
        };
        out.entry_states.push(AccountEntryStateRow {
            account_id: *id,
            signer_keys: d.signers.iter().map(|(k, _, _)| k.clone()).collect(),
            signer_weights: d.signers.iter().map(|(_, w, _)| *w).collect(),
            signer_types: d.signers.iter().map(|(_, _, t)| t.to_string()).collect(),
            master_weight: d.thresholds[0],
            threshold_low: d.thresholds[1],
            threshold_med: d.thresholds[2],
            threshold_high: d.thresholds[3],
            flags: d.flags,
            last_updated_ledger: i64::from(e.ledger),
        });
    }

    Ok(out)
}

async fn insert_chunked<T>(sink: &Sink, table: &str, rows: &[T]) -> Result<(), BackfillError>
where
    T: clickhouse::Row + clickhouse::RowOwned + serde::Serialize,
{
    for chunk in rows.chunks(INSERT_CHUNK) {
        insert_rows(sink.client(), table, chunk).await?;
    }
    Ok(())
}

/// The seed. Without `--execute`: reads its inputs from ClickHouse, decodes
/// the snapshot, folds, writes artifacts, inserts NOTHING. With `--execute`:
/// additionally inserts the four row sets.
pub async fn seed_command(
    sink: &Sink,
    artifacts: &Path,
    pinned_manifest: Option<&Path>,
    execute: bool,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    std::fs::create_dir_all(artifacts)
        .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", artifacts.display())))?;

    // Dry-run/execute drift is absorbed by the `>= checkpoint` guard — churned
    // rows are left alone, never corrected differently — so the pin is
    // optional here and exists for exact reproduction (the ADR 0056 LP merge
    // re-derives this seed's snapshot from `artifacts/manifest.json`).
    let (list, mut state) = snapshot::open_snapshot(
        pinned_manifest,
        if execute { " [EXECUTE]" } else { " [dry-run]" },
    )
    .await?;

    // Provenance artifact: the exact bucket list this run decoded. The archive
    // is content-addressed, so this manifest alone re-derives the identical
    // snapshot later (the LP-merge pass will need exactly that).
    let manifest = serde_json::json!({
        "checkpoint_ledger": list.checkpoint_ledger,
        "archive": PUBNET_ARCHIVE,
        "buckets": list.hashes,
    });
    std::fs::write(
        artifacts.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("static json"),
    )
    .map_err(|e| BackfillError::Incomplete(format!("write manifest: {e}")))?;

    // `GROUP BY id` collapses the RMT duplicates prod tables carry unmerged.
    let known_assets = fetch_id_set(sink, "SELECT id FROM assets GROUP BY id").await?;
    let known_accounts = fetch_id_set(sink, "SELECT id FROM accounts GROUP BY id").await?;
    println!(
        "  known dimension ids: {} assets, {} accounts",
        known_assets.len(),
        known_accounts.len()
    );
    // A short read is indistinguishable from a real one to everything
    // downstream: fewer known ids means more "absent" ids means more stubs.
    // These floors are far below the measured populations (344,989 assets /
    // 14.5M accounts as of 2026-08-18) — they catch a wrong database, not a
    // shrinking network.
    const MIN_ASSET_IDS: usize = 100_000;
    const MIN_ACCOUNT_IDS: usize = 5_000_000;
    if known_assets.len() < MIN_ASSET_IDS || known_accounts.len() < MIN_ACCOUNT_IDS {
        return Err(BackfillError::Incomplete(format!(
            "dimension id read looks wrong ({} assets, {} accounts; expected at least \
             {MIN_ASSET_IDS} and {MIN_ACCOUNT_IDS}) — is this the production database?",
            known_assets.len(),
            known_accounts.len()
        )));
    }

    let mut ghost_log = Vec::new();
    let mut report = Report::new(list.checkpoint_ledger);
    let corr = build_corrections(
        sink,
        &mut state,
        &known_assets,
        &known_accounts,
        list.checkpoint_ledger,
        &mut report,
        &mut ghost_log,
    )
    .await?;

    // The ghost list is the anomaly REPORT the policy demands — corrected in
    // the same run, but never silently.
    std::fs::write(artifacts.join("ghosts.tsv"), ghost_log.join("\n") + "\n")
        .map_err(|e| BackfillError::Incomplete(format!("write ghosts: {e}")))?;

    // The summary IS the four-way comparison — the same eleven buckets per
    // population the report renders, from one `Report`, plus
    // what this run would insert. An operator signs off on one document.
    report.write_dumps(&artifacts.join("dumps"))?;
    // Excluded on purpose — reported so the pass never reads as exhaustive
    // when it is not. Contract-held classic balances live in the SAC's
    // `ContractData`, not a trustline, so the snapshot's trustline set would
    // call every one of them a phantom; type-3 is the same reason, different
    // entry type; pool shares are the same ledger entry type but live in
    // `lp_positions` on our side (ADR 0056 merges them).
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
        .query(
            "SELECT count() FROM balances \
             WHERE asset_id IN (SELECT id FROM assets WHERE asset_type = 3)",
        )
        .fetch_one()
        .await?;

    let summary = format!(
        "checkpoint {}\n{}{}{}\n  NOT COMPARED (deliberate, see module docs)\n    \
         contract-held classic rows  {:>12}\n    \
         type-3 Soroban rows         {:>12}\n    \
         snapshot pool shares        {:>12}  (our side: lp_positions)\n\
         \n  CORRECTIONS{}\n    \
         balances rows         {:>12}\n    \
         account_entry_state   {:>12}\n    \
         asset stubs           {:>12}\n    \
         account stubs         {:>12}\n\
         \n  ghosts.tsv holds every positive-amount row this run zeroes.\n",
        list.checkpoint_ledger,
        report.classic.render("CLASSIC CREDIT trustlines", false),
        report
            .native
            .render("NATIVE XLM holdings (AccountEntry, not a trustline)", true),
        report.render_missing_histogram(),
        excluded_contract,
        excluded_type3,
        state.live_pool_shares(),
        if execute {
            " — INSERTING"
        } else {
            " — dry-run, nothing inserted"
        },
        corr.balances.len(),
        corr.entry_states.len(),
        corr.asset_stubs.len(),
        corr.account_stubs.len(),
    );
    std::fs::write(artifacts.join("summary.txt"), &summary)
        .map_err(|e| BackfillError::Incomplete(format!("write summary: {e}")))?;
    println!("\n{summary}");

    if execute {
        println!("  inserting…");
        insert_chunked(sink, "assets", &corr.asset_stubs).await?;
        insert_chunked(sink, "accounts", &corr.account_stubs).await?;
        insert_chunked(sink, "balances", &corr.balances).await?;
        insert_chunked(sink, "account_entry_state", &corr.entry_states).await?;
        println!("  inserts done.");
    } else {
        println!("  dry-run: nothing inserted. Re-run with --execute to write.");
    }
    println!("  total {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}
