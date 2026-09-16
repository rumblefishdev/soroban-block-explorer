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
//! | `claimable_balance_holdings`, same four kinds (task 0210, [`claimable`]) | not yet measured | as above | as above |
//! | classic pools missing or stale on our side: `liquidity_pools` + `liquidity_pool_snapshots`, insert-only (task 0210, [`pools`]) | not yet measured | the entry's own ledger | — |
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
use crate::snapshot::archive::PUBNET_ARCHIVE;
use crate::snapshot::balances;
use crate::snapshot::claimable;
use crate::snapshot::dumps;
use crate::snapshot::network_state::{self, NetworkState};
use crate::snapshot::pools;
use crate::snapshot::report::Report;
use crate::snapshot::slices::key_slices;
use crate::util::insert_rows;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{AccountEntryStateRow, AccountRow, AssetRow};

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
    balances: balances::BalanceCorrections,
    entry_states: Vec<AccountEntryStateRow>,
    asset_stubs: Vec<AssetRow>,
    account_stubs: Vec<AccountRow>,
    claimable: claimable::ClaimableCorrections,
    pools: pools::PoolCorrections,
    dangling: Dangling,
}

/// References a seeded row introduces that resolve to no dimension row —
/// neither already in ClickHouse nor stubbed by this run.
///
/// The three sites that produce these were silent `continue`s: an id the
/// snapshot references but carries no identity for is simply not stubbed, and
/// the balance pointing at it goes in anyway. That is the failure mode the
/// stub pass exists to prevent, so it is counted, reported, and (for the two
/// that break a balance) refused at `--execute` rather than skipped.
#[derive(Default)]
struct Dangling {
    /// A seeded balance points at this asset; nothing will define it.
    assets: u64,
    /// A seeded balance belongs to this holder; nothing will define it.
    holders: u64,
    /// An asset stub names this issuer; nothing will define it. Cosmetic — it
    /// blanks the issuer column, it never orphans a balance — and legitimate:
    /// an issuer may merge while trustlines to its asset outlive it.
    issuers: u64,
}

/// Fetch the set of existing dimension ids straight from ClickHouse — the
/// tool reads its own inputs, like every other corrective command here.
///
/// Sliced on `id`, for the same reason [`stream_our_rows`] is. The unsliced
/// version read `accounts` in one query and timed out on the third production
/// run: 14.58M ids do not fit the operator profile's `max_execution_time = 30`,
/// which counts the time spent SENDING rows, not just aggregating them (the
/// aggregation alone measures 0.4s). It had succeeded twice before that, which
/// is the worst kind of limit — one that depends on how busy the server is.
///
/// Failure was loud, and stayed loud: a cursor error propagates, so a partial
/// id set can never be mistaken for a real one. That matters more here than
/// almost anywhere else in the run, because fewer known ids means more ids
/// judged absent, which means more dimension stubs — a truncated read would
/// manufacture rows for entities that already exist.
async fn fetch_id_set(sink: &Sink, table: &str) -> Result<HashSet<i64>, BackfillError> {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct IdRow {
        id: i64,
    }
    let mut out = HashSet::new();
    for (from, to) in key_slices() {
        // `GROUP BY id` collapses the RMT duplicates prod tables carry
        // unmerged. The HashSet would dedup anyway; doing it server-side keeps
        // ~8% of rows off the wire, which is the resource actually constrained.
        let sql = format!("SELECT id FROM {table} WHERE id BETWEEN {from} AND {to} GROUP BY id");
        let mut cursor = sink.client().query(&sql).fetch::<IdRow>()?;
        while let Some(r) = cursor.next().await? {
            out.insert(r.id);
        }
    }
    Ok(out)
}

/// Build every correction. Deterministic function of (snapshot state, our
/// rows as read, dimension id sets) — a re-run against the same inputs
/// produces identical rows and RMT collapses them.
async fn build_corrections(
    sink: &Sink,
    state: &mut NetworkState,
    known_assets: &HashSet<i64>,
    known_accounts: &HashSet<i64>,
    checkpoint: u32,
    report: &mut Report,
) -> Result<Corrections, BackfillError> {
    let mut out = Corrections::default();

    // Passes 1 and 2: `balances` against the snapshot's accounts and trustlines.
    let mut referenced_assets: HashSet<i64> = HashSet::new();
    let mut referenced_holders: HashSet<i64> = HashSet::new();
    out.balances = balances::build_corrections(
        sink,
        state,
        checkpoint,
        report,
        &mut referenced_assets,
        &mut referenced_holders,
    )
    .await?;
    // Pass 2b: `claimable_balance_holdings`, before the stubs its assets need.
    out.claimable =
        claimable::build_corrections(sink, state, checkpoint, report, &mut referenced_assets)
            .await?;
    // Pass 2c: classic pools with no current snapshot of ours, same reason.
    out.pools = pools::build_corrections(sink, state, checkpoint, &mut referenced_assets).await?;

    // Pass 3: dimension stubs — a seeded balance whose asset or holder has no
    // dimension row would render as a broken join, i.e. a new lie replacing an
    // old one. Issuers of stubbed assets count as referenced accounts too.
    // Issuers are kept apart from balance holders: a missing issuer stub blanks
    // a column, a missing holder stub orphans a balance. Only the second is a
    // reason to refuse the write, so they cannot share a counter.
    let mut referenced_issuers: HashSet<i64> = HashSet::new();
    for asset_id in &referenced_assets {
        if known_assets.contains(asset_id) {
            continue;
        }
        let Some((code, issuer)) = state.asset_registry.get(asset_id) else {
            // The snapshot references this asset but carries no live trustline
            // to recover its (code, issuer) from — so the balance above points
            // at an `assets` row that will never exist.
            out.dangling.assets += 1;
            continue;
        };
        let issuer_id = ids::account_id(issuer);
        // Built like live ingest builds it, so the id is recomputed from the
        // identity. An id that disagrees with the referenced one would define
        // some other asset and leave the reference dangling — count it so.
        let stub = AssetRow::staged(1, code.clone(), issuer_id, 0);
        if stub.id != *asset_id {
            out.dangling.assets += 1;
            continue;
        }
        out.asset_stubs.push(stub);
        referenced_issuers.insert(issuer_id);
    }
    // `union`, not `chain`: an issuer that also holds a balance appears in both
    // sets and would otherwise be stubbed twice.
    for holder_id in referenced_holders.union(&referenced_issuers) {
        if known_accounts.contains(holder_id) {
            continue;
        }
        let is_balance_holder = referenced_holders.contains(holder_id);
        let unresolved = |d: &mut Dangling| {
            if is_balance_holder {
                d.holders += 1;
            } else {
                d.issuers += 1;
            }
        };
        let Some(d) = state.account_details.get(holder_id) else {
            unresolved(&mut out.dangling); // referenced, not a live snapshot account
            continue;
        };
        // `get`, not `[]`: safe today only because `absorb` fills `accounts`
        // and `account_details` together, which is an invariant rather than a
        // type guarantee — and this path runs after 4.5 GB of decode.
        let Some(entry) = state.accounts.get(holder_id).copied() else {
            unresolved(&mut out.dangling);
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

/// Refuse `--execute` under a read-only identity, BEFORE the run costs
/// anything.
///
/// The failure it prevents is not subtle, it is just late: the first INSERT is
/// the last step of the pass, so a wrong certificate spends ~5 minutes and
/// 4.4 GB of archive download to learn a fact one SELECT already knows. The
/// operator laptop's cert maps to a `readonly = 1` user, which refuses an
/// INSERT before grants are consulted at all, so this is the ordinary case and
/// not an exotic one.
///
/// A READ, never a trial write: the decisive test for a write permission must
/// not itself be a write. `readonly` is the setting that actually gates the
/// insert — grants are never reached while it is 1 — so it is what gets
/// checked, rather than the user name, which is only reported to make the
/// message actionable.
async fn refuse_if_read_only(sink: &Sink) -> Result<(), BackfillError> {
    let (user, readonly): (String, u8) = sink
        .client()
        .query("SELECT currentUser(), toUInt8(getSetting('readonly'))")
        .fetch_one()
        .await?;
    if readonly != 0 {
        return Err(BackfillError::Incomplete(format!(
            "refusing --execute: connected as `{user}` with readonly = {readonly}, which \
             rejects every INSERT before grants are consulted. Use a write-capable \
             identity (see the write-identity section of docs/backfills.md); the \
             dry-run needs no change."
        )));
    }
    println!("  identity: {user} (readonly = 0) — writes permitted");
    Ok(())
}

/// The seed. Without `--execute`: reads its inputs from ClickHouse, decodes
/// the snapshot, folds, writes artifacts, inserts NOTHING. With `--execute`:
/// additionally inserts every correction the summary lists.
///
/// Order: everything that needs only the checkpoint ledger or ClickHouse —
/// write identity, writer coverage, the dimension id sets — runs BEFORE the
/// ~5-minute bucket download, so a refusal costs seconds.
pub async fn seed_command(
    sink: &Sink,
    artifacts_root: &Path,
    execute: bool,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();

    // Before anything expensive: a write we cannot make is worth knowing in
    // second 2, not in minute 11.
    if execute {
        refuse_if_read_only(sink).await?;
    }

    let list = network_state::latest_checkpoint().await?;
    let coverage = claimable::writer_coverage(
        claimable::first_writer_tombstone(sink).await?,
        list.checkpoint_ledger,
    );
    if let (true, Err(why)) = (execute, &coverage) {
        return Err(BackfillError::Incomplete(format!(
            "refusing --execute: {why}"
        )));
    }

    let known_assets = fetch_id_set(sink, "assets").await?;
    let known_accounts = fetch_id_set(sink, "accounts").await?;
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

    // One directory per checkpoint, so a run never overwrites the record of an
    // earlier one — `ghosts.tsv` is the only pre-image of what a run zeroed.
    let artifacts = &artifacts_root.join(list.checkpoint_ledger.to_string());
    std::fs::create_dir_all(artifacts)
        .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", artifacts.display())))?;
    println!("  artifacts → {}", artifacts.display());

    // Provenance artifact: the exact bucket list this run decoded. The archive
    // is content-addressed, so this manifest alone identifies the identical
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

    let (mut state, source_report) =
        network_state::open_snapshot(&list, if execute { " [EXECUTE]" } else { " [dry-run]" })
            .await?;

    let mut report = Report::new(list.checkpoint_ledger);
    let corr = build_corrections(
        sink,
        &mut state,
        &known_assets,
        &known_accounts,
        list.checkpoint_ledger,
        &mut report,
    )
    .await?;

    // The anomaly REPORTS the policy demands — corrected (or, for pools,
    // only listed) in the same run, but never silently.
    for (file, lines) in [
        ("ghosts.tsv", &corr.balances.ghosts),
        ("claimable_ghosts.tsv", &corr.claimable.ghosts),
        ("pools_gone.tsv", &corr.pools.gone_with_reserves),
    ] {
        std::fs::write(artifacts.join(file), lines.join("\n") + "\n")
            .map_err(|e| BackfillError::Incomplete(format!("write {file}: {e}")))?;
    }

    // The summary IS the comparison — the verdict buckets per population the
    // report renders, from one `Report`, plus every row this run would insert,
    // table by table in one block. An operator signs off on one document.
    report.write_dumps(&artifacts.join("dumps"))?;
    dumps::write_correction_dumps(
        &artifacts.join("dumps"),
        &corr.asset_stubs,
        &corr.account_stubs,
        &corr.entry_states,
        &state,
    )?;
    let (excluded_contract, excluded_type3) = balances::excluded_counts(sink).await?;

    let summary = format!(
        "checkpoint {}\n{}{}{}{}{}{}\n  NOT COMPARED (deliberate, see module docs)\n    \
         contract-held classic rows  {:>12}\n    \
         type-3 Soroban rows         {:>12}\n    \
         snapshot pool shares        {:>12}  (our side: lp_positions)\n\
         \n  CORRECTIONS{}\n    \
         balances                     {:>12}\n    \
         claimable_balance_holdings   {:>12}\n    \
         liquidity_pools              {:>12}\n    \
         liquidity_pool_snapshots     {:>12}\n    \
         account_entry_state          {:>12}\n    \
         assets (stubs)               {:>12}\n    \
         accounts (stubs)             {:>12}\n\
         \n  UNRESOLVED REFERENCES (must be 0 for the first two)\n    \
         assets a seeded balance points at  {:>12}\n    \
         holders a seeded balance is for    {:>12}\n    \
         issuers an asset stub names        {:>12}  (blanks a column, never a balance)\n\
         \n  ghosts.tsv holds every positive-amount row this run zeroes.\n  \
         dumps/asset_stubs.tsv holds EVERY new assets row, for offline audit.\n",
        list.checkpoint_ledger,
        source_report,
        report.classic.render("CLASSIC CREDIT trustlines", false),
        report
            .native
            .render("NATIVE XLM holdings (AccountEntry, not a trustline)", true),
        claimable::render_summary(&report, &coverage),
        pools::render_summary(&corr.pools),
        report.render_missing_histogram(),
        excluded_contract,
        excluded_type3,
        state.live_pool_shares(),
        if execute {
            " — INSERTING"
        } else {
            " — dry-run, nothing inserted"
        },
        corr.balances.rows.len(),
        corr.claimable.rows.len(),
        corr.pools.pool_rows.len(),
        corr.pools.snapshot_rows.len(),
        corr.entry_states.len(),
        corr.asset_stubs.len(),
        corr.account_stubs.len(),
        corr.dangling.assets,
        corr.dangling.holders,
        corr.dangling.issuers,
    );
    std::fs::write(artifacts.join("summary.txt"), &summary)
        .map_err(|e| BackfillError::Incomplete(format!("write summary: {e}")))?;
    println!("\n{summary}");

    if execute {
        // A balance whose asset or holder resolves to no dimension row renders
        // as a broken join — the "new lie replacing an old one" the stub pass
        // exists to prevent. The dry-run reports it; the write refuses it.
        if corr.dangling.assets > 0 || corr.dangling.holders > 0 {
            return Err(BackfillError::Incomplete(format!(
                "refusing to insert: {} seeded balances point at an undefined asset and {} at \
                 an undefined holder — every such row would render as a broken join. See the \
                 UNRESOLVED REFERENCES block in summary.txt",
                corr.dangling.assets, corr.dangling.holders
            )));
        }
        println!("  inserting…");
        insert_chunked(sink, "assets", &corr.asset_stubs).await?;
        insert_chunked(sink, "accounts", &corr.account_stubs).await?;
        insert_chunked(sink, "liquidity_pools", &corr.pools.pool_rows).await?;
        insert_chunked(sink, "liquidity_pool_snapshots", &corr.pools.snapshot_rows).await?;
        insert_chunked(sink, "balances", &corr.balances.rows).await?;
        insert_chunked(sink, claimable::TABLE, &corr.claimable.rows).await?;
        insert_chunked(sink, "account_entry_state", &corr.entry_states).await?;
        println!("  inserts done.");
    } else {
        println!("  dry-run: nothing inserted. Re-run with --execute to write.");
    }
    println!("  total {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}
