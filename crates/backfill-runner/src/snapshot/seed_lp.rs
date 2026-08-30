//! The LP checkpoint seed (task 0374, K4-6 follow-up) — the pass the balances
//! seed (task 0463, [`super::seed`]) deliberately left out: pool-share
//! trustlines live in `lp_positions`, not `balances`, and their pools in
//! `liquidity_pools`.
//!
//! ## Why it exists (measured 2026-08-29, chain-validated via getLedgerEntries)
//!
//! A holder whose pool-share trustline predates the ingest floor and was never
//! touched since has NO `lp_positions` row: 2,681 pools know <50% of their
//! shares' owners (1,164 of them live), 597 miss 1–50%. Zero overcounts — the
//! defect is one-sided. The same floor hides whole POOLS: a pre-floor pool
//! with no post-floor entry change has no `liquidity_pools` row either.
//!
//! ## What it writes (with `--execute`; without it, artifacts + counts only)
//!
//! | correction | version ledger | notes |
//! |---|---|---|
//! | missing live position | the ENTRY's own `lastModifiedLedgerSeq` | `first_deposit_ledger = 0` — the DOCUMENTED "unknown, predates our history" sentinel; the API reads it back as null |
//! | self-heal (snapshot newer than ours, amount differs) | the entry's own ledger | live-writer parity |
//! | missing `liquidity_pools` dimension row | the entry's own ledger | classic pair from the pool entry's own params |
//! | one `liquidity_pool_snapshots` row per stubbed pool | the entry's own ledger | reserves + total_shares from the entry — the pool stops rendering as stale-forever |
//! | `accounts` stubs for holders we lack | the entry's own ledger | same rule as the balances seed |
//!
//! **Ghosts (our positive pair, snapshot absent/dead) are REPORTED, not
//! corrected** — `ghosts.tsv` in the artifacts dir. The balances seed earned
//! its right to zero ghosts through a 100/100 RPC probe; this pass has no such
//! evidence yet, and the measured defect is one-sided (missing rows), so the
//! conservative half of the 0463 policy applies: anomaly report first.
//!
//! ## The versioning contract
//!
//! Everything versions on the entry's own `lastModifiedLedgerSeq` — never a
//! window boundary (the task 0492 defect) and never the checkpoint ledger for
//! live data. The live writer's newer rows then win regardless of load order,
//! and a seeded row can never outversion a real post-checkpoint write.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::archive::PUBNET_ARCHIVE;
use crate::snapshot::network_state;
use crate::snapshot::seed::{fetch_id_set, insert_chunked, refuse_if_read_only};
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{
    AccountRow, LiquidityPoolRow, LiquidityPoolSnapshotRow, LpPositionRow,
};

/// Floor on the our-rows read — same rationale as the balances seed's
/// `MIN_OUR_ROWS`: a short read (wrong database) is indistinguishable from a
/// real one and every missing row becomes a phantom gap this pass would
/// INSERT. Measured population 2026-08-29: 108,864 distinct pairs.
const MIN_OUR_PAIRS: usize = 80_000;

/// One of our deduplicated `lp_positions` pairs.
#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct OurLpRow {
    pool_hex: String,
    account_id: i64,
    /// Decimal128(7) scaled to its raw integer form — the same unit the
    /// XDR trustline `balance` carries (stroop-sized), so the two sides
    /// compare without float.
    shares_raw: i128,
    last_updated_ledger: i64,
    /// Carried for self-heals: the RMT replaces WHOLE rows, so a corrective
    /// row must restate the pair's real first deposit or erase it.
    first_deposit_ledger: i64,
}

#[derive(Default)]
struct Corrections {
    positions: Vec<LpPositionRow>,
    pool_stubs: Vec<LiquidityPoolRow>,
    snapshot_stubs: Vec<LiquidityPoolSnapshotRow>,
    account_stubs: Vec<AccountRow>,
    /// Our positive pairs the snapshot does not have (absent or dead) —
    /// reported, never corrected (see module doc).
    ghosts: Vec<String>,
    /// Live snapshot positions whose amount differs from ours AND whose entry
    /// ledger is newer — corrected to the snapshot value (live-writer parity).
    self_heals: usize,
    /// Live snapshot positions at balance 0 — a trustline open at zero. Not
    /// seeded (our convention writes zero-share rows only with lifecycle
    /// stamps the snapshot cannot supply); counted so the summary stays total.
    zero_skipped: usize,
    /// Seeded positions whose holder resolves to no accounts row and no
    /// snapshot detail — would render as a dropped participant, so `--execute`
    /// refuses while this is non-zero.
    dangling_holders: u64,
    /// Positions referencing a pool the snapshot has no live entry for
    /// (position outlived its pool in the same checkpoint — should be
    /// impossible; counted loudly rather than assumed away).
    poolless_positions: u64,
}

/// Read our deduplicated `lp_positions` pairs. One query — the table holds
/// ~109k pairs (vs the balances seed's 48.6M, which needed 64 slices).
async fn fetch_our_pairs(sink: &Sink) -> Result<HashMap<([u8; 32], i64), OurLpRow>, BackfillError> {
    // `argMax` collapses the unmerged RMT duplicates; the tuple keeps the
    // amount and its version from one row (same reasoning as `slice_sql`).
    let sql = "SELECT lower(hex(pool_id)) AS pool_hex, account_id, \
                      toInt128(tupleElement(best, 1) * 10000000) AS shares_raw, \
                      led AS last_updated_ledger, \
                      tupleElement(best, 2) AS first_deposit_ledger \
               FROM ( \
                   SELECT pool_id, account_id, \
                          argMax((shares, first_deposit_ledger), last_updated_ledger) AS best, \
                          max(last_updated_ledger) AS led \
                   FROM lp_positions \
                   GROUP BY pool_id, account_id \
               )";
    let mut out = HashMap::new();
    let mut cursor = sink.client().query(sql).fetch::<OurLpRow>()?;
    while let Some(row) = cursor.next().await? {
        let mut pool = [0u8; 32];
        hex::decode_to_slice(&row.pool_hex, &mut pool)
            .map_err(|e| BackfillError::Incomplete(format!("bad pool hex from CH: {e}")))?;
        out.insert((pool, row.account_id), row);
    }
    if out.len() < MIN_OUR_PAIRS {
        return Err(BackfillError::Incomplete(format!(
            "our lp_positions read returned {} pairs, expected at least {MIN_OUR_PAIRS} — \
             a short read reports our own positions as phantom gaps (wrong database?)",
            out.len()
        )));
    }
    Ok(out)
}

/// The set of pool ids `liquidity_pools` already has (both kinds — a hex id
/// collision across kinds is impossible, CAP-38 hashes vs contract payloads).
async fn fetch_known_pools(sink: &Sink) -> Result<HashSet<[u8; 32]>, BackfillError> {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct PoolHexRow {
        pool_hex: String,
    }
    let mut out = HashSet::new();
    let mut cursor = sink
        .client()
        .query("SELECT lower(hex(pool_id)) AS pool_hex FROM liquidity_pools GROUP BY pool_id")
        .fetch::<PoolHexRow>()?;
    while let Some(row) = cursor.next().await? {
        let mut pool = [0u8; 32];
        hex::decode_to_slice(&row.pool_hex, &mut pool)
            .map_err(|e| BackfillError::Incomplete(format!("bad pool hex from CH: {e}")))?;
        out.insert(pool);
    }
    Ok(out)
}

/// The LP seed. Without `--execute`: decode, fold, write artifacts, insert
/// NOTHING. With `--execute`: additionally insert the four row sets.
pub async fn seed_lp_command(
    sink: &Sink,
    artifacts_root: &Path,
    execute: bool,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    if execute {
        refuse_if_read_only(sink).await?;
    }

    let (list, state, source_report) =
        network_state::open_snapshot(if execute { " [EXECUTE]" } else { " [dry-run]" }).await?;

    let artifacts = &artifacts_root.join(format!("lp-{}", list.checkpoint_ledger));
    std::fs::create_dir_all(artifacts)
        .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", artifacts.display())))?;
    println!("  artifacts → {}", artifacts.display());
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

    let ours = fetch_our_pairs(sink).await?;
    let known_pools = fetch_known_pools(sink).await?;
    let known_accounts = fetch_id_set(sink, "accounts").await?;
    println!(
        "  our side: {} position pairs, {} pools, {} account ids",
        ours.len(),
        known_pools.len(),
        known_accounts.len()
    );

    let mut corr = Corrections::default();
    let mut matched: HashSet<([u8; 32], i64)> = HashSet::new();
    let mut referenced_holders: HashSet<i64> = HashSet::new();

    // Pass 1: snapshot positions against ours.
    for ((holder_id, pool_id), e) in &state.pool_shares {
        if !e.live {
            continue;
        }
        let key = (*pool_id, *holder_id);
        if e.balance == 0 {
            // Still marks the pair as present on chain, so a positive row of
            // ours does not read as a ghost.
            matched.insert(key);
            corr.zero_skipped += 1;
            continue;
        }
        match ours.get(&key) {
            Some(our) => {
                matched.insert(key);
                if i128::from(e.balance) != our.shares_raw
                    && i64::from(e.ledger) > our.last_updated_ledger
                {
                    corr.self_heals += 1;
                    corr.positions.push(LpPositionRow {
                        pool_id: *pool_id,
                        account_id: *holder_id,
                        shares: i128::from(e.balance),
                        // The pair exists on our side and the RMT replaces
                        // WHOLE rows — restate its real first deposit or a
                        // correction erases it.
                        first_deposit_ledger: our.first_deposit_ledger,
                        last_updated_ledger: i64::from(e.ledger),
                        closed_at_ledger: 0,
                    });
                }
            }
            None => {
                if !state.pools.get(pool_id).map(|p| p.live).unwrap_or(false) {
                    corr.poolless_positions += 1;
                }
                corr.positions.push(LpPositionRow {
                    pool_id: *pool_id,
                    account_id: *holder_id,
                    shares: i128::from(e.balance),
                    // 0 = "predates our history" — the documented sentinel the
                    // participants read maps to null, never a fabricated ledger.
                    first_deposit_ledger: 0,
                    last_updated_ledger: i64::from(e.ledger),
                    closed_at_ledger: 0,
                });
                referenced_holders.insert(*holder_id);
            }
        }
    }

    // Pass 2: ghosts — ours positive, snapshot has no live pair. Report only.
    for ((pool_id, account_id), our) in &ours {
        if our.shares_raw > 0 && !matched.contains(&(*pool_id, *account_id)) {
            corr.ghosts.push(format!(
                "{}\t{}\t{}\t{}",
                hex::encode(pool_id),
                account_id,
                our.shares_raw,
                our.last_updated_ledger
            ));
        }
    }

    // Pass 3: pool dimension stubs — every live pool entry we lack a row for,
    // plus one snapshot row from the entry itself so the pool does not render
    // as "no data ever". The classic pair comes from the entry's own params.
    for (pool_id, p) in &state.pools {
        if !p.live || known_pools.contains(pool_id) {
            continue;
        }
        let (a_type, a_code, a_issuer) = &p.asset_a;
        let (b_type, b_code, b_issuer) = &p.asset_b;
        corr.pool_stubs.push(LiquidityPoolRow {
            pool_id: *pool_id,
            asset_a_type: *a_type,
            asset_a_code: a_code.clone(),
            asset_a_issuer_id: if a_issuer.is_empty() {
                0
            } else {
                ids::account_id(a_issuer)
            },
            asset_b_type: *b_type,
            asset_b_code: b_code.clone(),
            asset_b_issuer_id: if b_issuer.is_empty() {
                0
            } else {
                ids::account_id(b_issuer)
            },
            fee_bps: p.fee_bps,
            last_updated_ledger: i64::from(p.ledger),
            pool_kind: 0,
            legs: Vec::new(),
            deployment_id: 0,
            pool_type_raw: String::new(),
            share_token_id: 0,
        });
        corr.snapshot_stubs.push(LiquidityPoolSnapshotRow {
            pool_id: *pool_id,
            ledger_sequence: i64::from(p.ledger),
            reserve_a: i128::from(p.reserve_a),
            reserve_b: i128::from(p.reserve_b),
            total_shares: i128::from(p.total_shares),
            tvl: None,
            volume: None,
            fee_revenue: None,
            gross_volume_a: None,
        });
    }

    // Pass 4: account stubs for seeded holders we lack (balances-seed rule:
    // a seeded position whose holder has no dimension row renders as a
    // DROPPED participant — the resolver filter_maps it away).
    for holder_id in &referenced_holders {
        if known_accounts.contains(holder_id) {
            continue;
        }
        let (Some(d), Some(entry)) = (
            state.account_details.get(holder_id),
            state.accounts.get(holder_id).copied(),
        ) else {
            corr.dangling_holders += 1;
            continue;
        };
        corr.account_stubs.push(AccountRow {
            id: *holder_id,
            account_id: d.strkey.clone(),
            first_seen_ledger: i64::from(entry.ledger),
            last_seen_ledger: i64::from(entry.ledger),
            sequence_number: d.seq_num,
            home_domain: (!d.home_domain.is_empty()).then(|| d.home_domain.clone()),
        });
    }

    std::fs::write(artifacts.join("ghosts.tsv"), corr.ghosts.join("\n") + "\n")
        .map_err(|e| BackfillError::Incomplete(format!("write ghosts: {e}")))?;

    // Protocol invariant, used as a built-in decode check: every pool entry
    // counts its own share trustlines, so the sum over live pools must equal
    // the live pool-share count this same decode produced. A mismatch means
    // the decoder dropped records — surface it in the summary, loudly.
    let chain_trustline_total: i64 = state
        .pools
        .values()
        .filter(|p| p.live)
        .map(|p| p.trustline_count)
        .sum();
    let summary = format!(
        "LP seed — checkpoint {}\n{source_report}\n  \
         invariant: sum(pool_shares_trust_line_count) = {} vs decoded live shares = {}{}\n  \
         snapshot: {} live pool shares, {} live pools\n  \
         our side: {} position pairs, {} pool rows\n\
         \n  CORRECTIONS{}\n    \
         lp_positions rows       {:>10}  (of which {} self-heals)\n    \
         liquidity_pools stubs   {:>10}\n    \
         snapshot stubs          {:>10}\n    \
         account stubs           {:>10}\n\
         \n  REPORTED, NOT CORRECTED\n    \
         ghosts (ours positive, chain has no pair)  {:>8}  → ghosts.tsv\n    \
         zero-balance trustlines skipped            {:>8}\n\
         \n  MUST BE ZERO for --execute\n    \
         dangling holders     {:>10}\n    \
         poolless positions   {:>10}\n",
        list.checkpoint_ledger,
        chain_trustline_total,
        state.live_pool_shares(),
        if chain_trustline_total == state.live_pool_shares() as i64 {
            "  OK"
        } else {
            "  !! MISMATCH — decoder dropped records, do not --execute"
        },
        state.live_pool_shares(),
        state.live_pools(),
        ours.len(),
        known_pools.len(),
        if execute {
            " — INSERTING"
        } else {
            " — dry-run, nothing inserted"
        },
        corr.positions.len(),
        corr.self_heals,
        corr.pool_stubs.len(),
        corr.snapshot_stubs.len(),
        corr.account_stubs.len(),
        corr.ghosts.len(),
        corr.zero_skipped,
        corr.dangling_holders,
        corr.poolless_positions,
    );
    std::fs::write(artifacts.join("summary.txt"), &summary)
        .map_err(|e| BackfillError::Incomplete(format!("write summary: {e}")))?;
    println!("\n{summary}");

    if execute {
        if corr.dangling_holders > 0 || corr.poolless_positions > 0 {
            return Err(BackfillError::Incomplete(format!(
                "refusing to insert: {} seeded positions have an unresolvable holder and {} \
                 reference a pool with no live entry — see summary.txt",
                corr.dangling_holders, corr.poolless_positions
            )));
        }
        println!("  inserting…");
        insert_chunked(sink, "accounts", &corr.account_stubs).await?;
        insert_chunked(sink, "liquidity_pools", &corr.pool_stubs).await?;
        insert_chunked(sink, "liquidity_pool_snapshots", &corr.snapshot_stubs).await?;
        insert_chunked(sink, "lp_positions", &corr.positions).await?;
        println!("  inserts done.");
    } else {
        println!("  dry-run: nothing inserted. Re-run with --execute to write.");
    }
    println!("  total {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}
