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
//! | `account_signers` full seed | every live account | the entry's own ledger | — |
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
use crate::snapshot::{self, PUBNET_ARCHIVE, SnapshotState, fetch_bucket_list};
use crate::util::insert_rows;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{AccountRow, AccountSignersRow, AssetRow, BalanceRow};

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
    signers: Vec<AccountSignersRow>,
    asset_stubs: Vec<AssetRow>,
    account_stubs: Vec<AccountRow>,
    n_missing: u64,
    n_closure: u64,
    /// Native ghosts separately from classic: their stroops sum IS an XLM
    /// figure, while summing classic amounts across different assets would be
    /// a unit-less nonsense number (the first dry-run printed 796 billion
    /// "XLM" that way — memecoin units masquerading as stroops).
    n_ghost_native: u64,
    ghost_native_stroops: i128,
    n_ghost_classic: u64,
    n_heal: u64,
    /// We stamped closed, the network says live at a newer ledger — re-opened.
    n_closed_but_live: u64,
    /// Same ledger, different amount. Counted and reported, never written.
    n_divergent_same_ledger: u64,
    /// Rows the snapshot does not have but WE touched after the checkpoint —
    /// the snapshot is the stale side, so they are left alone.
    n_newer_than_checkpoint: u64,
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

/// Fold one of our streamed rows into corrections, acting on the SHARED
/// verdict the compare pass reports. One rule, two consumers — the report an
/// operator signs off on predicts what `--execute` writes. (`--execute`
/// re-reads our rows fresh, like every corrective command here; churn since
/// the dry-run is absorbed by the `>= checkpoint` guard, so the drift is only
/// rows newly LEFT ALONE, never a different correction.)
fn fold_our_row(
    row: &snapshot::OurRow,
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
    match snapshot::verdict(row, snapshot::snap_entry_for(state, row), checkpoint) {
        // Nothing to write: both sides agree (Stale = equal amounts, our ledger
        // merely older — the verdict rule guarantees the equality), or our side
        // is the fresher one and the live parser already knows better.
        V::AlreadyClosed | V::Agree | V::DivergentOursNewer | V::Stale => {}
        // Same ledger, different amount — one parser is wrong and freshness
        // cannot arbitrate. Reported, never auto-healed: picking a winner would
        // bury the only evidence that something mis-parsed.
        V::DivergentSameLedger => out.n_divergent_same_ledger += 1,
        // We hid a holding the network says is live at a NEWER ledger. Re-open
        // it at the entry's own ledger, which outversions our wrong closure.
        V::ClosedButLive => {
            let Some(e) = snapshot::snap_entry_for(state, row) else {
                return;
            };
            out.n_closed_but_live += 1;
            out.balances.push(BalanceRow {
                holder_id,
                asset_id,
                amount: i128::from(e.amount),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
        }
        V::NewerThanCheckpoint => out.n_newer_than_checkpoint += 1,
        // The snapshot is strictly newer AND the amounts differ, so adopt its
        // amount at ITS ledger.
        V::HealFromSnapshot => {
            let Some(e) = snapshot::snap_entry_for(state, row) else {
                return;
            };
            out.n_heal += 1;
            out.balances.push(BalanceRow {
                holder_id,
                asset_id,
                amount: i128::from(e.amount),
                last_updated_ledger: i64::from(e.ledger),
                closed_at_ledger: 0,
            });
        }
        v @ (V::Closure | V::Ghost) => {
            if v == V::Closure {
                out.n_closure += 1;
            } else {
                if asset_id == ids::NATIVE_ASSET_ID {
                    out.n_ghost_native += 1;
                    out.ghost_native_stroops += amount;
                } else {
                    out.n_ghost_classic += 1;
                }
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
    ghost_log: &mut Vec<String>,
) -> Result<Corrections, BackfillError> {
    let mut out = Corrections::default();

    // Pass 1: our rows → closures, ghosts, self-heals. The shared streamed
    // read (with its short-read floor) — the same rows the compare pass sees.
    let rows_read = crate::snapshot_compare::stream_our_rows(sink, |row| {
        fold_our_row(row, state, checkpoint, &mut out, ghost_log);
    })
    .await?;
    println!("  folded {rows_read} of our rows");

    // Pass 2: unmatched live snapshot entries → missing-holding inserts.
    let mut referenced_assets: HashSet<i64> = HashSet::new();
    let mut referenced_holders: HashSet<i64> = HashSet::new();
    for (key, e) in &state.trustlines {
        if e.live && !e.matched {
            out.n_missing += 1;
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
            out.n_missing += 1;
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

    // Pass 4: signers — one row per live account, the FULL set, versioned on
    // the entry's own ledger so the (future) live writer wins on any change.
    for (id, e) in &state.accounts {
        if !e.live {
            continue;
        }
        let Some(d) = state.account_details.get(id) else {
            continue;
        };
        out.signers.push(AccountSignersRow {
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

    let http = crate::snapshot::archive_client()?;
    // Optional pin: decode a previously recorded snapshot instead of the
    // newest checkpoint. NOT required for `--execute` — the freshest
    // checkpoint is the better input, and it is complete by construction: the
    // archive's `.well-known` manifest is stellar-core's atomic commit point,
    // written LAST, so it only ever names fully uploaded buckets (verified
    // against docs/history.md and a live probe, 2026-08-21). Dry-run/execute
    // drift is absorbed by the `>= checkpoint` guard — churned rows are left
    // alone, never corrected differently. The pin exists for exact
    // reproduction: the ADR 0056 LP merge re-derives this seed's snapshot
    // from `artifacts/manifest.json`.
    let list = match pinned_manifest {
        Some(path) => crate::snapshot::bucket_list_from_manifest(path)?,
        None => fetch_bucket_list(&http, PUBNET_ARCHIVE).await?,
    };
    println!(
        "checkpoint ledger {} — {} buckets{}",
        list.checkpoint_ledger,
        list.hashes.len(),
        if execute { " [EXECUTE]" } else { " [dry-run]" }
    );

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

    let take = list.hashes.len();
    let mut state = crate::snapshot::build_state(
        &http,
        &list,
        SnapshotState::with_details(),
        |i, bytes, secs| {
            println!("  [{:>2}/{take}] {bytes:>10} B  {secs:>6.1}s", i + 1);
        },
    )
    .await?;
    println!(
        "  snapshot folded: {} accounts, {} trustlines, {} assets in registry",
        state.accounts.len(),
        state.trustlines.len(),
        state.asset_registry.len()
    );

    let mut ghost_log = Vec::new();
    let corr = build_corrections(
        sink,
        &mut state,
        &known_assets,
        &known_accounts,
        list.checkpoint_ledger,
        &mut ghost_log,
    )
    .await?;

    // The ghost list is the anomaly REPORT the policy demands — corrected in
    // the same run, but never silently.
    std::fs::write(artifacts.join("ghosts.tsv"), ghost_log.join("\n") + "\n")
        .map_err(|e| BackfillError::Incomplete(format!("write ghosts: {e}")))?;

    let summary = format!(
        "checkpoint {}\n\
         balances corrections: {} rows\n\
           missing inserted    {}\n\
           closures stamped    {}\n\
           native ghosts zeroed  {}   ({:.7} XLM — full list in ghosts.tsv)\n\
           classic ghosts zeroed {}   (amounts are per-asset units; see ghosts.tsv)\n\
           self-heals          {}\n\
           re-opened (we hid a live holding) {}\n\
           divergent SAME ledger (defect?)   {}   NOT written — investigate\n\
           left alone (ours newer than checkpoint) {}\n\
         account_signers rows: {}\n\
         asset stubs:          {}\n\
         account stubs:        {}\n",
        list.checkpoint_ledger,
        corr.balances.len(),
        corr.n_missing,
        corr.n_closure,
        corr.n_ghost_native,
        corr.ghost_native_stroops as f64 / 1e7,
        corr.n_ghost_classic,
        corr.n_heal,
        corr.n_closed_but_live,
        corr.n_divergent_same_ledger,
        corr.n_newer_than_checkpoint,
        corr.signers.len(),
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
        insert_chunked(sink, "account_signers", &corr.signers).await?;
        println!("  inserts done.");
    } else {
        println!("  dry-run: nothing inserted. Re-run with --execute to write.");
    }
    println!("  total {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}
