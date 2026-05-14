//! Initial-snapshot mechanism for CH `accounts` state on backfill
//! start (task 0214, audit §E06).
//!
//! ## What this fixes
//!
//! The CH writer's account-state staging consumes the parser-emitted
//! `ExtractedAccountState` slice produced by
//! `xdr_parser::extract_account_states`. That function fires only on
//! observed `LedgerEntryAccount` / `LedgerEntryTrustLine` changes —
//! so accounts that appear in the backfill window's
//! `transaction_participants` driver but never have their
//! `AccountEntry` updated inside the window persist as
//! `sequence_number=0, home_domain=null, 0 balances` skeletons.
//!
//! At 64 k-ledger windows that's the majority of participants. The
//! audit doc §E06 records a representative example:
//! `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55` stored
//! as a skeleton row while Horizon shows the account fully alive with
//! `seqnum=148e15`, `home_domain="ultracapital.xyz"`, 12 861 XLM.
//!
//! ## What this does
//!
//! Runs **once per backfill window**, **after** the per-ledger ingest
//! loop has populated `transaction_participants` (so the discovery
//! query has rows to scan). Steps:
//!
//! 1. Discover the set of G-StrKey accounts referenced in the window
//!    and currently sitting as skeletons (`sequence_number = 0`) in
//!    the CH `accounts` hub. Discovery joins `transaction_participants`
//!    against `accounts FINAL`, surfacing both the StrKey and the
//!    existing `first_seen_ledger` so the snapshot row can preserve
//!    the earliest observation rather than overwrite it.
//! 2. For each batch of ≤ [`crate::rpc_snapshot::MAX_KEYS_PER_REQUEST`]
//!    accounts, build `LedgerKey::Account` keys and POST to Soroban
//!    RPC's `getLedgerEntries`. Decode each returned `AccountEntry`
//!    via [`crate::rpc_snapshot::decode_account_snapshot`].
//! 3. Stage the decoded `AccountSnapshot`s into the CH writer's
//!    `accounts` / `account_balances_current` insert paths.
//!
//! ## How the snapshot wins under `ReplacingMergeTree`
//!
//! `accounts` is `ReplacingMergeTree(last_seen_ledger)` — the
//! background merger keeps the row with the highest version per
//! `ORDER BY (account_id)`. The parser-driven path on the per-ledger
//! ingest loop stamps `last_seen_ledger = participant_ledger`
//! (`participant_ledger ∈ [start, end]`). To guarantee the snapshot
//! row wins under `FINAL` reads, we stamp it with
//! `last_seen_ledger = end + 1` — strictly greater than any in-window
//! stub, while still inside the contiguous ledger range (the next
//! window starts at `end + 1` and its participant rows are at
//! `≥ end + 1` too, so subsequent snapshots / parser writes for the
//! same account take over naturally as the indexer marches forward).
//!
//! `first_seen_ledger` is carried through from the existing skeleton
//! row — overwriting it with the window watermark would erase the
//! earliest observation of the account in this CH dataset.
//!
//! ## Phase 2 incremental top-up gate
//!
//! On a window-resume (operator re-runs backfill against a partially
//! populated CH), discovery skips accounts that already have
//! `sequence_number > 0` in the live state — those are
//! parser-emitted real-data rows that the snapshot should not
//! clobber. The gate lives in [`collect_skeleton_accounts`] (Phase 2
//! reuses the same SELECT query, just with a different post-filter).
//!
//! ## Phase 3 trustline + asset aggregate
//!
//! Out of scope per the task body — once Phase 1 lands, the
//! `account_balances_current` table is reliably populated so a
//! future task (the CH port of `recompute_asset_aggregates`, task
//! 0194) can derive `assets.holder_count` / `total_supply`. The
//! trustline-side enrichment lives as a follow-up note in the task
//! body §"Out of Scope".
//!
//! ## Behaviour when no RPC endpoint is configured
//!
//! The runner CLI exposes `--soroban-rpc-url`. If unset, the
//! bootstrap step logs `info!("bootstrap skipped — no rpc endpoint")`
//! and returns `Ok(0)`. PG-target runs always skip the bootstrap
//! (PG's account-state population is independent — task 0119 +
//! ADR 0027 §7 cover it via a different path).

use std::collections::HashSet;

use clickhouse::Client as ClickhouseClient;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::error::BackfillError;
use crate::rpc_snapshot::{
    AccountSnapshot, RpcClient, account_ledger_key, decode_account_snapshot,
};
use crate::sink::Sink;

/// Aggregate counters logged at the end of the bootstrap step. Useful
/// for the operator to spot "RPC returned 0 entries for 1 000 keys"
/// type failures without trawling per-batch logs.
#[derive(Debug, Default, Clone, Copy)]
pub struct BootstrapStats {
    pub discovered: usize,
    pub fetched: usize,
    pub staged_accounts: usize,
    pub rpc_batches: usize,
    pub rpc_errors: usize,
}

/// Run the bootstrap step for the window `[start, end]`.
///
/// CH-only — PG target short-circuits to `Ok(default)` and logs the
/// skip. No-op when `rpc_url` is `None` (lets the runner be exercised
/// without Soroban RPC connectivity in CI / dev).
pub async fn bootstrap_account_state(
    sink: &Sink,
    rpc_url: Option<&str>,
    start: u32,
    end: u32,
) -> Result<BootstrapStats, BackfillError> {
    let Sink::Clickhouse(client) = sink else {
        debug!("bootstrap_account_state: skipped (PG target)");
        return Ok(BootstrapStats::default());
    };
    let Some(rpc_url) = rpc_url else {
        info!(
            start,
            end, "bootstrap_account_state skipped — no --soroban-rpc-url configured"
        );
        return Ok(BootstrapStats::default());
    };

    let rpc = match RpcClient::new(rpc_url) {
        Ok(c) => c,
        Err(err) => {
            warn!(%err, "bootstrap_account_state: RPC client build failed; skipping");
            return Ok(BootstrapStats::default());
        }
    };

    let accounts = collect_skeleton_accounts(client, start, end).await?;
    let discovered = accounts.len();
    info!(
        start,
        end, discovered, "bootstrap_account_state: discovered skeleton accounts"
    );
    if discovered == 0 {
        return Ok(BootstrapStats {
            discovered,
            ..BootstrapStats::default()
        });
    }

    let mut stats = BootstrapStats {
        discovered,
        ..BootstrapStats::default()
    };
    let mut snapshots: Vec<AccountSnapshot> = Vec::with_capacity(discovered);
    // Build per-account `first_seen_ledger` lookup so the snapshot row
    // can preserve the existing earliest observation rather than reset
    // it to the window watermark.
    let first_seen_by_account: std::collections::HashMap<String, i64> = accounts
        .iter()
        .map(|a| (a.account_id.clone(), a.first_seen_ledger))
        .collect();
    // Build the keys, dropping any malformed StrKeys early.
    let keys: Vec<_> = accounts
        .iter()
        .filter_map(|a| account_ledger_key(&a.account_id))
        .collect();
    stats.rpc_batches = keys
        .len()
        .div_ceil(crate::rpc_snapshot::MAX_KEYS_PER_REQUEST);

    match rpc.get_ledger_entries(&keys).await {
        Ok(entries) => {
            stats.fetched = entries.len();
            for entry in entries {
                if let Some(snap) = decode_account_snapshot(&entry.data) {
                    snapshots.push(snap);
                }
            }
        }
        Err(err) => {
            stats.rpc_errors += 1;
            warn!(%err, "bootstrap_account_state: RPC fetch failed; bailing out");
            // Don't fail the whole run — bootstrap is opportunistic.
            return Ok(stats);
        }
    }

    stats.staged_accounts =
        stage_account_snapshots(client, &snapshots, &first_seen_by_account, end).await?;
    info!(
        discovered = stats.discovered,
        fetched = stats.fetched,
        staged = stats.staged_accounts,
        batches = stats.rpc_batches,
        rpc_errors = stats.rpc_errors,
        "bootstrap_account_state: completed"
    );
    Ok(stats)
}

/// Row shape for the discovery query — pairs each skeleton account's
/// StrKey with its existing `first_seen_ledger` so the snapshot row can
/// preserve the earliest observation. Private to this module.
#[derive(Debug, Deserialize, clickhouse::Row)]
struct AccountSkeletonRow {
    account_id: String,
    first_seen_ledger: i64,
}

/// Discover G-StrKey accounts that are referenced in the window but
/// have **no live state** in CH yet (or only a skeleton row with
/// `sequence_number = 0`).
///
/// Joins `transaction_participants` (the participant universe) against
/// the `accounts` hub, picking out the rows where the FK landing is
/// still a skeleton. Phase 2's incremental top-up gate is the
/// `sequence_number = 0` filter on the JOIN — it lets a re-run of the
/// same window only top up the rows that need it.
///
/// Empty result is a normal outcome (e.g. fresh CH database where the
/// participants table is itself empty before the per-ledger ingest
/// runs). Caller logs and returns from the bootstrap step.
async fn collect_skeleton_accounts(
    client: &ClickhouseClient,
    start: u32,
    end: u32,
) -> Result<Vec<AccountSkeletonRow>, BackfillError> {
    // The participants table holds Int64 surrogate IDs; the natural
    // G-StrKey lives on the `accounts` hub. Join them to recover the
    // StrKey + the existing `first_seen_ledger` (so the snapshot row
    // can preserve the earliest observation), with a `WHERE` clause
    // that picks only skeletons.
    //
    // `FINAL` on `accounts` ensures we read the most-recent
    // sequence_number / home_domain / first_seen_ledger
    // (RMT(last_seen_ledger)). Without it the JOIN can see a stale
    // skeleton even though a newer populated row exists.
    //
    // `min(a.first_seen_ledger)` defends against the (rare) case where
    // FINAL has not yet collapsed a duplicate skeleton — we still keep
    // the earliest observation.
    let query = "
        SELECT a.account_id AS account_id,
               min(a.first_seen_ledger) AS first_seen_ledger
          FROM transaction_participants AS tp
          JOIN accounts AS a FINAL ON a.id = tp.account_id
         WHERE tp.ledger_sequence BETWEEN ? AND ?
           AND a.sequence_number = 0
         GROUP BY a.account_id
    ";
    let rows = client
        .query(query)
        .bind(i64::from(start))
        .bind(i64::from(end))
        .fetch_all::<AccountSkeletonRow>()
        .await
        .map_err(BackfillError::Ch)?;
    let mut seen: HashSet<String> = HashSet::with_capacity(rows.len());
    let mut out: Vec<AccountSkeletonRow> = Vec::with_capacity(rows.len());
    for r in rows {
        if r.account_id.is_empty() || !r.account_id.starts_with('G') {
            continue;
        }
        if seen.insert(r.account_id.clone()) {
            out.push(r);
        }
    }
    Ok(out)
}

/// Insert one row per decoded `AccountSnapshot` into the `accounts`
/// hub (RMT(last_seen_ledger), so the newest version wins on
/// background merge).
///
/// Key choices:
///
/// - **`last_seen_ledger = end + 1`** — strictly greater than any
///   in-window participant skeleton (which carry
///   `last_seen_ledger ∈ [start, end]`), so the snapshot wins under
///   `FINAL` reads and on the background merge. The `+1` is still
///   inside the contiguous ledger range — the next backfill window
///   starts at `end + 1` and its rows are also at `≥ end + 1`, so
///   subsequent snapshots / parser writes take over naturally as the
///   indexer marches forward.
/// - **`first_seen_ledger` carried through from the existing skeleton
///   row** (passed in via `first_seen_by_account`) — overwriting it
///   with the window watermark would erase the earliest observation
///   of the account in this CH dataset.
///
/// Companion native-balance rows land in `account_balances_current`
/// keyed on `(account_id, asset_type=Native, '', 0)` so reads see
/// the snapshot's native XLM balance. The balance row uses
/// `last_updated_ledger = end + 1` for the same RMT-version reason.
///
/// Returns the count of `accounts` rows written. Per-batch insert
/// errors propagate as `BackfillError::Ch`.
async fn stage_account_snapshots(
    client: &ClickhouseClient,
    snapshots: &[AccountSnapshot],
    first_seen_by_account: &std::collections::HashMap<String, i64>,
    end: u32,
) -> Result<usize, BackfillError> {
    if snapshots.is_empty() {
        return Ok(0);
    }
    // `end + 1` keeps us above every participant skeleton in this
    // window while staying inside the contiguous ledger range. Cast
    // through u64 so we never overflow `u32 + 1`.
    let watermark: i64 = i64::try_from(u64::from(end) + 1).unwrap_or(i64::MAX);

    // accounts insert
    let mut accounts_insert = client
        .insert::<db_clickhouse::persist::rows::AccountRow>("accounts")
        .await
        .map_err(BackfillError::Ch)?;
    for s in snapshots {
        let id = db_clickhouse::persist::ids::account_id(&s.account_id);
        // Preserve the existing earliest observation. If we somehow
        // staged a snapshot for an account the discovery query didn't
        // surface (defensive — should never happen because keys are
        // built from `accounts`), fall back to the watermark.
        let first_seen_ledger = first_seen_by_account
            .get(&s.account_id)
            .copied()
            .unwrap_or(watermark);
        accounts_insert
            .write(&db_clickhouse::persist::rows::AccountRow {
                id,
                account_id: s.account_id.clone(),
                first_seen_ledger,
                last_seen_ledger: watermark,
                sequence_number: s.sequence_number,
                home_domain: s.home_domain.clone(),
            })
            .await
            .map_err(BackfillError::Ch)?;
    }
    accounts_insert.end().await.map_err(BackfillError::Ch)?;

    // account_balances_current insert — native row per snapshot.
    let mut balances_insert = client
        .insert::<db_clickhouse::persist::rows::AccountBalanceRow>("account_balances_current")
        .await
        .map_err(BackfillError::Ch)?;
    for s in snapshots {
        let account_id = db_clickhouse::persist::ids::account_id(&s.account_id);
        // Native XLM balance: stroops → Decimal128(7) raw is exactly
        // the i64 stroop value cast to i128 (same 10⁷ scale).
        let balance: i128 = i128::from(s.balance);
        balances_insert
            .write(&db_clickhouse::persist::rows::AccountBalanceRow {
                account_id,
                asset_type: 0, // AssetType::Native
                asset_code: String::new(),
                issuer_id: 0,
                balance,
                last_updated_ledger: watermark,
            })
            .await
            .map_err(BackfillError::Ch)?;
    }
    balances_insert.end().await.map_err(BackfillError::Ch)?;

    Ok(snapshots.len())
}

#[cfg(test)]
mod tests {
    //! Bootstrap-level tests.
    //!
    //! ## Coverage layers
    //!
    //! 1. **Pure-decoder unit tests** — live in `rpc_snapshot::tests`
    //!    next to the code under test. Lock the XDR → snapshot field
    //!    contract.
    //! 2. **PG short-circuit** — no env vars, runs everywhere.
    //! 3. **CH-gated smoke** — `CLICKHOUSE_URL` set, exercises
    //!    `collect_skeleton_accounts` against real CH (no RPC needed —
    //!    we fabricate a skeleton row and verify discovery picks it up).
    //! 4. **End-to-end with a live RPC** — open as operational
    //!    follow-up under the task's empirical-replay AC.
    use super::*;
    use crate::sink::Sink;
    use db_clickhouse::persist::rows::{AccountRow, TransactionParticipantRow};

    /// Far above any realistic Soroban sequence — fits in u32 and i64,
    /// keeps fixtures out of the way of any real data on a shared CH.
    /// Mirrors the constant in `sink::tests` so a parallel sink test
    /// can't collide with this one's window.
    const TEST_BASE: u32 = 4_000_010_000;

    #[tokio::test]
    async fn pg_target_short_circuits() {
        // Build a placeholder PG sink — we never connect; the function
        // pattern-matches on the variant and exits before touching it.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://noop")
            .expect("lazy connect must succeed without I/O");
        let sink = Sink::Postgres(pool);
        let stats = bootstrap_account_state(&sink, Some("http://does-not-matter"), 0, 1)
            .await
            .expect("PG short-circuit must not error");
        assert_eq!(stats.discovered, 0);
        assert_eq!(stats.fetched, 0);
        assert_eq!(stats.staged_accounts, 0);
    }

    async fn build_ch_sink() -> Option<Sink> {
        let url = std::env::var("CLICKHOUSE_URL").ok()?;
        let cfg = db_clickhouse::Config {
            url,
            ..db_clickhouse::Config::from_env()
        };
        let client = db_clickhouse::client(&cfg);
        if let Err(err) = db_clickhouse::apply_init_sql(&client).await {
            eprintln!("CLICKHOUSE_URL set but apply_init_sql failed ({err}) — skipping");
            return None;
        }
        Some(Sink::Clickhouse(client))
    }

    /// Fabricate a "skeleton" account row + a participant row referencing
    /// it. The discovery query should then return exactly this StrKey.
    /// Tests the Phase 2 incremental top-up gate
    /// (`WHERE sequence_number = 0`) without touching Soroban RPC.
    #[tokio::test]
    async fn clickhouse_collect_skeleton_accounts_picks_up_zero_seqnum_rows() {
        let Some(sink) = build_ch_sink().await else {
            eprintln!("CLICKHOUSE_URL not set — skipping");
            return;
        };
        let Sink::Clickhouse(ref client) = sink else {
            unreachable!()
        };

        let ledger = TEST_BASE + 5;
        let strkey = "GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55";
        let already_filled_strkey = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        let skel_id = db_clickhouse::persist::ids::account_id(strkey);
        let filled_id = db_clickhouse::persist::ids::account_id(already_filled_strkey);

        // Cleanup any prior fixture in this window.
        for tbl in ["accounts", "transaction_participants"] {
            let _ = client
                .query(&format!(
                    "ALTER TABLE {tbl} DELETE WHERE \
                     ({} = ? OR {} = ?) {}",
                    if tbl == "accounts" {
                        "id"
                    } else {
                        "account_id"
                    },
                    if tbl == "accounts" {
                        "id"
                    } else {
                        "account_id"
                    },
                    if tbl == "transaction_participants" {
                        format!("AND ledger_sequence BETWEEN {ledger} AND {ledger}")
                    } else {
                        String::new()
                    }
                ))
                .bind(skel_id)
                .bind(filled_id)
                .with_setting("mutations_sync", "1")
                .execute()
                .await;
        }

        // Insert one skeleton + one already-filled account row.
        let mut accounts = client
            .insert::<AccountRow>("accounts")
            .await
            .expect("open accounts insert");
        accounts
            .write(&AccountRow {
                id: skel_id,
                account_id: strkey.to_string(),
                first_seen_ledger: i64::from(ledger),
                last_seen_ledger: i64::from(ledger),
                sequence_number: 0,
                home_domain: None,
            })
            .await
            .expect("write skeleton row");
        accounts
            .write(&AccountRow {
                id: filled_id,
                account_id: already_filled_strkey.to_string(),
                first_seen_ledger: i64::from(ledger),
                last_seen_ledger: i64::from(ledger),
                sequence_number: 99, // already filled — must be skipped
                home_domain: Some("example.com".into()),
            })
            .await
            .expect("write filled row");
        accounts.end().await.expect("close accounts insert");

        // Both accounts appear in transaction_participants for this ledger.
        let mut parts = client
            .insert::<TransactionParticipantRow>("transaction_participants")
            .await
            .expect("open participants insert");
        parts
            .write(&TransactionParticipantRow {
                account_id: skel_id,
                ledger_sequence: i64::from(ledger),
                transaction_id: 1,
            })
            .await
            .expect("write skel participant");
        parts
            .write(&TransactionParticipantRow {
                account_id: filled_id,
                ledger_sequence: i64::from(ledger),
                transaction_id: 1,
            })
            .await
            .expect("write filled participant");
        parts.end().await.expect("close participants insert");

        // Discovery must return only the skeleton.
        let got = collect_skeleton_accounts(client, ledger, ledger)
            .await
            .expect("discovery must succeed");
        let got_strkeys: Vec<String> = got.iter().map(|r| r.account_id.clone()).collect();
        assert!(
            got_strkeys.contains(&strkey.to_string()),
            "skeleton account must be discovered; got={got_strkeys:?}"
        );
        assert!(
            !got_strkeys.contains(&already_filled_strkey.to_string()),
            "already-filled account must be skipped (Phase 2 gate); got={got_strkeys:?}"
        );
        // The skeleton's first_seen_ledger comes back through the
        // discovery query — bootstrap relies on it to preserve the
        // earliest observation on snapshot insert.
        let skel = got
            .iter()
            .find(|r| r.account_id == strkey)
            .expect("skeleton row present");
        assert_eq!(
            skel.first_seen_ledger,
            i64::from(ledger),
            "discovery must carry skeleton's first_seen_ledger through",
        );

        // Cleanup.
        for tbl in ["accounts", "transaction_participants"] {
            let _ = client
                .query(&format!(
                    "ALTER TABLE {tbl} DELETE WHERE \
                     ({} = ? OR {} = ?)",
                    if tbl == "accounts" {
                        "id"
                    } else {
                        "account_id"
                    },
                    if tbl == "accounts" {
                        "id"
                    } else {
                        "account_id"
                    },
                ))
                .bind(skel_id)
                .bind(filled_id)
                .with_setting("mutations_sync", "1")
                .execute()
                .await;
        }
    }

    /// Stage a fixed `AccountSnapshot` and verify it lands in
    /// `accounts` + `account_balances_current` with the expected
    /// field values. Locks the snapshot → row mapping.
    #[tokio::test]
    async fn clickhouse_stage_account_snapshots_writes_rows() {
        let Some(sink) = build_ch_sink().await else {
            eprintln!("CLICKHOUSE_URL not set — skipping");
            return;
        };
        let Sink::Clickhouse(ref client) = sink else {
            unreachable!()
        };

        // Use a deterministic StrKey not used by any other smoke.
        let strkey = "GAAA3DLOXJWE74P3GAAQNVGPS5DKWWZRBP5DSE74RGD3UPNYI2NXTUYC";
        let watermark = TEST_BASE + 20;
        let id = db_clickhouse::persist::ids::account_id(strkey);

        // Cleanup any prior rows.
        for q in [
            "ALTER TABLE accounts DELETE WHERE id = ?",
            "ALTER TABLE account_balances_current DELETE WHERE account_id = ?",
        ] {
            let _ = client
                .query(q)
                .bind(id)
                .with_setting("mutations_sync", "1")
                .execute()
                .await;
        }

        let snap = AccountSnapshot {
            account_id: strkey.into(),
            sequence_number: 123_456_789,
            balance: 7_500_000_000, // 750 XLM in stroops
            home_domain: Some("ultracapital.xyz".into()),
        };
        // Mirror production: pre-populate the `first_seen_by_account`
        // map so the staged row preserves the existing observation
        // ledger rather than collapsing it onto the watermark.
        let pre_existing_first_seen = i64::from(watermark - 5);
        let mut first_seen_by_account = std::collections::HashMap::new();
        first_seen_by_account.insert(strkey.to_string(), pre_existing_first_seen);
        let staged = stage_account_snapshots(client, &[snap], &first_seen_by_account, watermark)
            .await
            .expect("stage must succeed");
        assert_eq!(staged, 1);

        // Read back the account row with the four fields the
        // bootstrap is responsible for: sequence + home_domain come
        // from the snapshot; first_seen_ledger preserves the existing
        // skeleton's value; last_seen_ledger is the RMT version stamp.
        #[derive(Deserialize, clickhouse::Row)]
        struct AcctReadback {
            sequence_number: i64,
            home_domain: Option<String>,
            first_seen_ledger: i64,
            last_seen_ledger: i64,
        }
        let acct: AcctReadback = client
            .query(
                "SELECT sequence_number, home_domain, first_seen_ledger, last_seen_ledger
                   FROM accounts FINAL WHERE id = ? LIMIT 1",
            )
            .bind(id)
            .fetch_one()
            .await
            .expect("read account row");
        assert_eq!(acct.sequence_number, 123_456_789);
        assert_eq!(acct.home_domain.as_deref(), Some("ultracapital.xyz"));
        assert_eq!(
            acct.first_seen_ledger, pre_existing_first_seen,
            "snapshot must preserve the existing earliest observation",
        );
        // RMT-version stamp: `end + 1`, not `start`, so the snapshot
        // beats any in-window participant skeleton under FINAL.
        let expected_last_seen = i64::from(watermark) + 1;
        assert_eq!(
            acct.last_seen_ledger, expected_last_seen,
            "snapshot last_seen_ledger must be `end + 1` to win RMT version",
        );

        // Read back the native balance row.
        #[derive(Deserialize, clickhouse::Row)]
        struct BalReadback {
            balance: i128,
        }
        let bal: BalReadback = client
            .query(
                "SELECT balance FROM account_balances_current FINAL \
                 WHERE account_id = ? AND asset_type = 0 LIMIT 1",
            )
            .bind(id)
            .fetch_one()
            .await
            .expect("read balance row");
        assert_eq!(bal.balance, 7_500_000_000);

        // Cleanup.
        for q in [
            "ALTER TABLE accounts DELETE WHERE id = ?",
            "ALTER TABLE account_balances_current DELETE WHERE account_id = ?",
        ] {
            let _ = client
                .query(q)
                .bind(id)
                .with_setting("mutations_sync", "1")
                .execute()
                .await;
        }
    }
}
