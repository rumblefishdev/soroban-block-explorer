//! Tier-1 rebuild tests. Same gating posture as `bootstrap.rs` and
//! `sink.rs` — skipped without `CLICKHOUSE_URL`. Each test cleans up
//! its fixture range before and after to stay safe on a shared CH.
use super::*;
use crate::sink::Sink;
use db_clickhouse::persist::rows::{AccountRow, TransactionParticipantRow};

const TEST_BASE: u32 = 4_000_020_000;

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
    Some(Sink::new(client))
}

/// Dry-run smoke for the accounts rebuild: stamp an account with a
/// deliberately-wrong `first_seen_ledger` (RMT collapse outcome),
/// stamp two `transaction_participants` rows at earlier ledgers,
/// invoke `rebuild_accounts` in dry-run mode. Asserts the live
/// `accounts` table is UNCHANGED — the staging-table swap only
/// happens on a real run.
///
/// This test deliberately does NOT verify staging contents: the
/// dry-run path drops the staging table inside `finalize()` before
/// the test can query it. Real-run correctness (staging actually
/// contains the corrected MIN) is exercised by
/// `clickhouse_rebuild_accounts_real_run_writes_corrected_min`
/// below.
#[tokio::test]
async fn clickhouse_rebuild_accounts_dry_run_leaves_live_untouched() {
    let Some(sink) = build_ch_sink().await else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let client = sink.client();

    let strkey = "GCQFXHQUTKDRRTPRDB7RH3FNRRJUQB3FA3KZGY42PXTH3FRWXWCATXFE";
    let acct_id = db_clickhouse::persist::ids::account_id(strkey);
    let early = i64::from(TEST_BASE + 5);
    let later = i64::from(TEST_BASE + 50);

    // Cleanup any prior fixture.
    for q in [
        "ALTER TABLE accounts DELETE WHERE id = ?",
        "ALTER TABLE transaction_participants DELETE WHERE account_id = ?",
        "DROP TABLE IF EXISTS accounts_staging_repair_tier1",
    ] {
        let _ = client
            .query(q)
            .bind(acct_id)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }

    // Plant a "collapsed" row: first_seen_ledger says `later` even
    // though earlier participation should set it to `early`.
    let mut accounts = client
        .insert::<AccountRow>("accounts")
        .await
        .expect("open accounts insert");
    accounts
        .write(&AccountRow {
            id: acct_id,
            account_id: strkey.to_string(),
            first_seen_ledger: later,
            last_seen_ledger: later,
            sequence_number: 1,
            home_domain: None,
        })
        .await
        .expect("write account fixture");
    accounts.end().await.expect("close accounts insert");

    // Two participant rows: one at `early`, one at `later`.
    let mut parts = client
        .insert::<TransactionParticipantRow>("transaction_participants")
        .await
        .expect("open participants insert");
    parts
        .write(&TransactionParticipantRow {
            account_id: acct_id,
            ledger_sequence: early,
            application_order: 1,
        })
        .await
        .expect("write early participant");
    parts
        .write(&TransactionParticipantRow {
            account_id: acct_id,
            ledger_sequence: later,
            application_order: 2,
        })
        .await
        .expect("write later participant");
    parts.end().await.expect("close participants insert");

    // Dry-run: builds the staging table, logs row count, then drops
    // staging in `finalize()`. We can only check the count returned
    // and that the live table is untouched — staging contents are
    // gone by the time this assertion runs.
    let rows = rebuild_accounts(client, /* dry_run */ true)
        .await
        .expect("rebuild_accounts must succeed");
    assert!(rows > 0, "staging must contain rows");

    #[derive(Debug, serde::Deserialize, clickhouse::Row)]
    struct AcctFsl {
        first_seen_ledger: i64,
    }
    let row: AcctFsl = client
        .query("SELECT first_seen_ledger FROM accounts FINAL WHERE id = ? LIMIT 1")
        .bind(acct_id)
        .fetch_one()
        .await
        .expect("read account row after dry-run");
    assert_eq!(
        row.first_seen_ledger, later,
        "dry-run must not touch the live table"
    );

    // Cleanup.
    for q in [
        "ALTER TABLE accounts DELETE WHERE id = ?",
        "ALTER TABLE transaction_participants DELETE WHERE account_id = ?",
        "DROP TABLE IF EXISTS accounts_staging_repair_tier1",
    ] {
        let _ = client
            .query(q)
            .bind(acct_id)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }
}

/// Real-run end-to-end: same fixture as the dry-run test, but
/// invokes `rebuild_accounts(client, dry_run=false)` so the staging
/// table is built AND EXCHANGE'd with `accounts`. Asserts the live
/// `accounts.first_seen_ledger` for the fixture account flips from
/// `later` (planted) to `early` (the actual MIN of participants).
///
/// This is the proof that the rebuild logic produces correct
/// values — the dry-run test only proves "doesn't touch live".
///
/// Uses a different StrKey from the dry-run test so the two can run
/// independently (test-threads=1 still recommended on shared CH).
#[tokio::test]
async fn clickhouse_rebuild_accounts_real_run_writes_corrected_min() {
    let Some(sink) = build_ch_sink().await else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let client = sink.client();

    // Distinct StrKey from the dry-run test so concurrent runs don't
    // share a fixture row.
    let strkey = "GAQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQQ";
    let acct_id = db_clickhouse::persist::ids::account_id(strkey);
    let early = i64::from(TEST_BASE + 7);
    let later = i64::from(TEST_BASE + 70);

    // Cleanup any prior fixture.
    for q in [
        "ALTER TABLE accounts DELETE WHERE id = ?",
        "ALTER TABLE transaction_participants DELETE WHERE account_id = ?",
        "DROP TABLE IF EXISTS accounts_staging_repair_tier1",
    ] {
        let _ = client
            .query(q)
            .bind(acct_id)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }

    // Plant collapsed row with WRONG first_seen_ledger.
    let mut accounts = client
        .insert::<AccountRow>("accounts")
        .await
        .expect("open accounts insert");
    accounts
        .write(&AccountRow {
            id: acct_id,
            account_id: strkey.to_string(),
            first_seen_ledger: later,
            last_seen_ledger: later,
            sequence_number: 1,
            home_domain: None,
        })
        .await
        .expect("write account fixture");
    accounts.end().await.expect("close accounts insert");

    // Two participant rows at `early` and `later` ledgers.
    let mut parts = client
        .insert::<TransactionParticipantRow>("transaction_participants")
        .await
        .expect("open participants insert");
    parts
        .write(&TransactionParticipantRow {
            account_id: acct_id,
            ledger_sequence: early,
            application_order: 11,
        })
        .await
        .expect("write early participant");
    parts
        .write(&TransactionParticipantRow {
            account_id: acct_id,
            ledger_sequence: later,
            application_order: 12,
        })
        .await
        .expect("write later participant");
    parts.end().await.expect("close participants insert");

    // REAL RUN — staging built, EXCHANGE TABLES swaps it with
    // `accounts`. After this, FINAL read of the fixture account
    // returns the corrected first_seen_ledger.
    let rows = rebuild_accounts(client, /* dry_run */ false)
        .await
        .expect("rebuild_accounts real-run must succeed");
    assert!(rows > 0, "swap must have touched at least the fixture row");

    #[derive(Debug, serde::Deserialize, clickhouse::Row)]
    struct AcctFsl {
        first_seen_ledger: i64,
        last_seen_ledger: i64,
        sequence_number: i64,
    }
    let row: AcctFsl = client
        .query("SELECT first_seen_ledger, last_seen_ledger, sequence_number FROM accounts FINAL WHERE id = ? LIMIT 1")
        .bind(acct_id)
        .fetch_one()
        .await
        .expect("read account row after real run");
    assert_eq!(
        row.first_seen_ledger, early,
        "real run must REPLACE first_seen_ledger with MIN(participants.ledger_sequence) = {early}"
    );
    // Other columns must be preserved (RMT version + non-rebuilt fields).
    assert_eq!(
        row.last_seen_ledger, later,
        "last_seen_ledger must pass through unchanged"
    );
    assert_eq!(
        row.sequence_number, 1,
        "sequence_number must pass through unchanged"
    );

    // Cleanup. Note: after EXCHANGE, the original `accounts` table
    // is renamed to `accounts_staging_repair_tier1` and dropped by
    // finalize(). The current `accounts` is the rebuilt staging.
    for q in [
        "ALTER TABLE accounts DELETE WHERE id = ?",
        "ALTER TABLE transaction_participants DELETE WHERE account_id = ?",
        "DROP TABLE IF EXISTS accounts_staging_repair_tier1",
    ] {
        let _ = client
            .query(q)
            .bind(acct_id)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }
}
