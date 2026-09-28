//! ClickHouse-backed check that a quiet pool's providers keep their share.
//!
//! A classic pool writes a snapshot only when its ledger entry changes, so a
//! pool nobody traded for weeks has an old snapshot that is still its current
//! state. The participants read used to take `total_shares` only from a
//! snapshot within 7 days of the chain tip, which blanked the share of 41% of
//! pools with providers on production (task 0374). This runs the real
//! `init.sql` and the real query in a throwaway database.
//! Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api quiet_pool_participants_keep_their_share

use super::*;

const DB: &str = "api_test_0374_participants_share";

const POOL: &str = "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2";
const ACCOUNT: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

#[tokio::test]
async fn quiet_pool_participants_keep_their_share() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping quiet-pool share check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");

    // The pool's only snapshot is at ledger 1,000; the chain tip is 1,000,000 —
    // far past any 7-day window (120,960 ledgers). One provider holds a quarter.
    for sql in [
        "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) VALUES \
             (1000000, unhex(repeat('00', 32)), now64(3), 23, 0, 100)"
            .to_string(),
        format!(
            "INSERT INTO liquidity_pool_snapshots (pool_id, ledger_sequence, reserve_a, reserve_b, total_shares) VALUES \
             (unhex('{POOL}'), 1000, 10, 10, 400)"
        ),
        format!(
            "INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger) VALUES \
             (unhex('{POOL}'), 42, 100, 1000, 1000)"
        ),
        format!(
            "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) VALUES \
             (42, '{ACCOUNT}', 1000, 1000, 1)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let rows = fetch_participants(&ch, POOL, None, 10, Direction::Next)
        .await
        .expect("participants query runs");

    assert_eq!(rows.len(), 1, "the provider is listed");
    let pct: f64 = rows[0]
        .share_percentage
        .as_deref()
        .expect("a quiet pool's provider keeps a share percentage")
        .parse()
        .expect("percentage is a decimal string");
    assert!(
        (pct - 25.0).abs() < 1e-9,
        "100 of 400 shares is 25%, got {pct}"
    );

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
