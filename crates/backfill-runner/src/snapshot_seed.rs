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
//! the exported known-id set, so they never contend with an existing row.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::Path;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::{BucketList, HoldingKey, PUBNET_ARCHIVE, SnapshotState, fetch_bucket_list};
use crate::util::insert_rows;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{AccountRow, AccountSignersRow, AssetRow, BalanceRow};

/// Insert batch size. RowBinary streams; this only bounds peak buffering.
const INSERT_CHUNK: usize = 500_000;

/// All correction rows, still in memory, ready to insert or count.
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
    /// Rows the snapshot does not have but WE touched after the checkpoint —
    /// the snapshot is the stale side, so they are left alone.
    n_newer_than_checkpoint: u64,
}

/// Read a one-column TSV of existing dimension ids (`SELECT id FROM …`).
fn read_id_set(path: &Path) -> Result<HashSet<i64>, BackfillError> {
    let file = std::fs::File::open(path)
        .map_err(|e| BackfillError::Incomplete(format!("open {}: {e}", path.display())))?;
    let mut out = HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|e| BackfillError::Incomplete(format!("read ids: {e}")))?;
        if line.is_empty() {
            continue;
        }
        out.insert(
            line.trim()
                .parse::<i64>()
                .map_err(|e| BackfillError::Incomplete(format!("id '{line}': {e}")))?,
        );
    }
    Ok(out)
}

/// Fold one of our exported rows into corrections. Mirrors the compare pass's
/// classification 1:1 — the shape RPC verification confirmed.
/// One row of our export, parsed.
struct OurRow {
    holder_id: i64,
    asset_id: i64,
    amount: i128,
    last_updated: i64,
    closed_at: i64,
}

fn fold_our_row(
    row: &OurRow,
    state: &mut SnapshotState,
    checkpoint: u32,
    out: &mut Corrections,
    ghost_log: &mut Vec<String>,
) {
    let OurRow {
        holder_id,
        asset_id,
        amount,
        last_updated,
        closed_at,
    } = *row;
    if closed_at != 0 {
        return; // already stamped by the writer — nothing to correct
    }
    let entry = if asset_id == ids::NATIVE_ASSET_ID {
        state.accounts.get_mut(&holder_id)
    } else {
        state.trustlines.get_mut(&HoldingKey {
            holder_id,
            asset_id,
        })
    };
    match entry {
        Some(e) if e.live => {
            e.matched = true;
            // Self-heal only where the snapshot is STRICTLY newer; ties and
            // newer-ours mean the live parser already knows better.
            if i128::from(e.amount) != amount && i64::from(e.ledger) > last_updated {
                out.n_heal += 1;
                out.balances.push(BalanceRow {
                    holder_id,
                    asset_id,
                    amount: i128::from(e.amount),
                    last_updated_ledger: i64::from(e.ledger),
                    closed_at_ledger: 0,
                });
            }
        }
        other => {
            if let Some(e) = other {
                e.matched = true;
            }
            // The snapshot's silence is only evidence AS OF its checkpoint. A
            // row our writer touched AFTER that ledger is fresher than the
            // snapshot: closing it would delete a holding the network created
            // in the gap. Skip and report — never guess against newer data.
            if last_updated > i64::from(checkpoint) {
                out.n_newer_than_checkpoint += 1;
                return;
            }
            if amount == 0 {
                out.n_closure += 1;
            } else {
                if asset_id == ids::NATIVE_ASSET_ID {
                    out.n_ghost_native += 1;
                    out.ghost_native_stroops += amount;
                } else {
                    out.n_ghost_classic += 1;
                }
                ghost_log.push(format!("{holder_id}\t{asset_id}\t{amount}\t{last_updated}"));
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

/// Build every correction. Pure function of (snapshot state, our export,
/// dimension id sets) — deterministic, so a re-run against the same inputs
/// produces identical rows and RMT collapses them.
fn build_corrections(
    state: &mut SnapshotState,
    our_rows: &Path,
    known_assets: &HashSet<i64>,
    known_accounts: &HashSet<i64>,
    checkpoint: u32,
    ghost_log: &mut Vec<String>,
) -> Result<Corrections, BackfillError> {
    let mut out = Corrections::default();

    // Pass 1: our rows → closures, ghosts, self-heals.
    let file = std::fs::File::open(our_rows)
        .map_err(|e| BackfillError::Incomplete(format!("open {}: {e}", our_rows.display())))?;
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|e| BackfillError::Incomplete(format!("read rows: {e}")))?;
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            return Err(BackfillError::Incomplete(format!(
                "our-rows line has {} fields, want 5",
                f.len()
            )));
        }
        let parse_err =
            |e: std::num::ParseIntError| BackfillError::Incomplete(format!("our-rows parse: {e}"));
        let row = OurRow {
            holder_id: f[0].parse().map_err(parse_err)?,
            asset_id: f[1].parse().map_err(parse_err)?,
            amount: f[2].parse().map_err(parse_err)?,
            last_updated: f[3].parse().map_err(parse_err)?,
            closed_at: f[4].parse().map_err(parse_err)?,
        };
        fold_our_row(&row, state, checkpoint, &mut out, ghost_log);
    }

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
        let entry = state.accounts[holder_id];
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

/// The seed. Without `--execute`: decodes, folds, writes artifacts, inserts
/// NOTHING. With `--execute`: additionally inserts the four row sets.
pub async fn seed_command(
    sink: &Sink,
    our_rows: &Path,
    assets_ids: &Path,
    accounts_ids: &Path,
    artifacts: &Path,
    execute: bool,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    std::fs::create_dir_all(artifacts)
        .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", artifacts.display())))?;

    let http = reqwest::Client::new();
    let list = fetch_bucket_list(&http, PUBNET_ARCHIVE).await?;
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

    let known_assets = read_id_set(assets_ids)?;
    let known_accounts = read_id_set(accounts_ids)?;
    println!(
        "  known dimension ids: {} assets, {} accounts",
        known_assets.len(),
        known_accounts.len()
    );

    let mut state = SnapshotState::with_details();
    for (i, hash) in list.hashes.iter().enumerate() {
        let url = BucketList::url(PUBNET_ARCHIVE, hash);
        let t0 = std::time::Instant::now();
        crate::snapshot::stream_bucket_from_url(&http, &url, |rec| {
            state.absorb_record(&rec);
            Ok(())
        })
        .await?;
        println!(
            "  [{:>2}/{}] {:>6.1}s  {}",
            i + 1,
            list.hashes.len(),
            t0.elapsed().as_secs_f64(),
            &hash[..12]
        );
    }
    println!(
        "  snapshot folded: {} accounts, {} trustlines, {} assets in registry",
        state.accounts.len(),
        state.trustlines.len(),
        state.asset_registry.len()
    );

    let mut ghost_log = Vec::new();
    let corr = build_corrections(
        &mut state,
        our_rows,
        &known_assets,
        &known_accounts,
        list.checkpoint_ledger,
        &mut ghost_log,
    )?;

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
