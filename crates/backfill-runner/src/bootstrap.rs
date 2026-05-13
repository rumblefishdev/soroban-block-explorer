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
//! Runs **once per backfill window** (Phase 1, Step 1 of the task)
//! before the per-ledger ingest loop. Steps:
//!
//! 1. Discover the set of G-StrKey accounts referenced in the window.
//!    Phase 1 source: scan the CH `transaction_participants` table for
//!    distinct `account_id` rows in `[start, end]`. (In a fresh
//!    backfill that table is empty — Phase 1 logs `discovered=0` and
//!    returns; the runner then proceeds to per-ledger ingest, after
//!    which a Phase 2 top-up pass could re-run this step. Phase 2 is
//!    a future refinement, gated on operational need.)
//! 2. For each batch of ≤ [`crate::rpc_snapshot::MAX_KEYS_PER_REQUEST`]
//!    accounts, build `LedgerKey::Account` keys and POST to Soroban
//!    RPC's `getLedgerEntries`. Decode each returned `AccountEntry`
//!    via [`crate::rpc_snapshot::decode_account_snapshot`].
//! 3. Stage the decoded `AccountSnapshot`s into the CH writer's
//!    `accounts` / `account_balances_current` insert paths, tagged
//!    with `from_snapshot = true` provenance for telemetry.
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
            end,
            "bootstrap_account_state skipped — no --soroban-rpc-url configured"
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
        end,
        discovered,
        "bootstrap_account_state: discovered skeleton accounts"
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
    // Build the keys, dropping any malformed StrKeys early.
    let keys: Vec<_> = accounts
        .iter()
        .filter_map(|a| account_ledger_key(a))
        .collect();
    stats.rpc_batches = keys.len().div_ceil(crate::rpc_snapshot::MAX_KEYS_PER_REQUEST);

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

    stats.staged_accounts = stage_account_snapshots(client, &snapshots, start).await?;
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

/// Row shape for the discovery query against CH
/// `transaction_participants`. Private to this module.
#[derive(Debug, Deserialize, clickhouse::Row)]
struct ParticipantRow {
    account_id: String,
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
) -> Result<Vec<String>, BackfillError> {
    // The participants table holds Int64 surrogate IDs; the natural
    // G-StrKey lives on the `accounts` hub. Join them to recover the
    // StrKey, with a `WHERE` clause that picks only skeletons.
    //
    // `FINAL` on `accounts` ensures we read the most-recent
    // sequence_number / home_domain (RMT(last_seen_ledger)). Without
    // it the JOIN can see a stale skeleton even though a newer
    // populated row exists.
    let query = "
        SELECT DISTINCT a.account_id AS account_id
          FROM transaction_participants AS tp
          JOIN accounts FINAL AS a ON a.id = tp.account_id
         WHERE tp.ledger_sequence BETWEEN ? AND ?
           AND a.sequence_number = 0
    ";
    let rows = client
        .query(query)
        .bind(i64::from(start))
        .bind(i64::from(end))
        .fetch_all::<ParticipantRow>()
        .await
        .map_err(BackfillError::Ch)?;
    let mut set: HashSet<String> = HashSet::with_capacity(rows.len());
    for r in rows {
        if !r.account_id.is_empty() && r.account_id.starts_with('G') {
            set.insert(r.account_id);
        }
    }
    Ok(set.into_iter().collect())
}

/// Insert one row per decoded `AccountSnapshot` into the `accounts`
/// hub (RMT, so the newest `last_seen_ledger` wins on background
/// merge). `last_seen_ledger` is fixed to the window's `start` —
/// snapshot reflects state at that ledger boundary by RPC contract.
///
/// Companion native-balance rows land in `account_balances_current`
/// keyed on `(account_id, asset_type=Native, '', 0)` so reads see
/// the snapshot's native XLM balance.
///
/// Returns the count of `accounts` rows written. Per-batch insert
/// errors propagate as `BackfillError::Ch`.
async fn stage_account_snapshots(
    client: &ClickhouseClient,
    snapshots: &[AccountSnapshot],
    start: u32,
) -> Result<usize, BackfillError> {
    if snapshots.is_empty() {
        return Ok(0);
    }
    let watermark = i64::from(start);

    // accounts insert
    let mut accounts_insert = client
        .insert::<db_clickhouse::persist::rows::AccountRow>("accounts")
        .await
        .map_err(BackfillError::Ch)?;
    for s in snapshots {
        let id = db_clickhouse::persist::ids::account_id(&s.account_id);
        accounts_insert
            .write(&db_clickhouse::persist::rows::AccountRow {
                id,
                account_id: s.account_id.clone(),
                first_seen_ledger: watermark,
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
                    if tbl == "accounts" { "id" } else { "account_id" },
                    if tbl == "accounts" { "id" } else { "account_id" },
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
        assert!(
            got.contains(&strkey.to_string()),
            "skeleton account must be discovered; got={got:?}"
        );
        assert!(
            !got.contains(&already_filled_strkey.to_string()),
            "already-filled account must be skipped (Phase 2 gate); got={got:?}"
        );

        // Cleanup.
        for tbl in ["accounts", "transaction_participants"] {
            let _ = client
                .query(&format!(
                    "ALTER TABLE {tbl} DELETE WHERE \
                     ({} = ? OR {} = ?)",
                    if tbl == "accounts" { "id" } else { "account_id" },
                    if tbl == "accounts" { "id" } else { "account_id" },
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
        let staged = stage_account_snapshots(client, &[snap], watermark)
            .await
            .expect("stage must succeed");
        assert_eq!(staged, 1);

        // Read back the account row.
        #[derive(Deserialize, clickhouse::Row)]
        struct AcctReadback {
            sequence_number: i64,
            home_domain: Option<String>,
        }
        let acct: AcctReadback = client
            .query(
                "SELECT sequence_number, home_domain FROM accounts FINAL WHERE id = ? LIMIT 1",
            )
            .bind(id)
            .fetch_one()
            .await
            .expect("read account row");
        assert_eq!(acct.sequence_number, 123_456_789);
        assert_eq!(acct.home_domain.as_deref(), Some("ultracapital.xyz"));

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
