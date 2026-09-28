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

const SOROBAN_DB: &str = "api_test_0374_soroban_participants";
const SOROBAN_POOL: &str = "d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3";
const NO_TOKEN_POOL: &str = "e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4";
const EMPTY_POOL: &str = "f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5f5";
const PARTIAL_POOL: &str = "b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7";
const UNREAD_POOL: &str = "a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6";
const SHARE_TOKEN: &str = "CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW";
const GAUGE: &str = "CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB";

/// A soroban pool's providers are its share token's holders — accounts and
/// contracts — scaled by the token's decimals, each page dividing by the
/// whole, the pool's own stored total where it keeps one. A pool with no
/// share token is "not indexed" (`None`), never an empty list.
#[tokio::test]
async fn soroban_participants_are_share_token_holders() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban participants check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {SOROBAN_DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {SOROBAN_DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(SOROBAN_DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");

    // Token 77 held by account 42 (300, then an older 999 version) and
    // contract 43 (100); a zero balance (44) is not a provider.
    for sql in [
        format!(
            "INSERT INTO pool_instance_state (pool_id, plane_id, share_token_id, total_shares, derived_at_ledger) VALUES \
             (unhex('{SOROBAN_POOL}'), 1, 77, 4000000000, 10), (unhex('{NO_TOKEN_POOL}'), 1, 0, 0, 10), \
             (unhex('{EMPTY_POOL}'), 1, 88, 0, 10), (unhex('{UNREAD_POOL}'), 1, 99, 500, 10), \
             (unhex('{PARTIAL_POOL}'), 1, 77, 9000000000, 10)"
        ),
        "INSERT INTO balances (holder_id, asset_id, amount, last_updated_ledger) VALUES \
             (42, 77, 9990000000, 5), (42, 77, 3000000000, 20), (43, 77, 1000000000, 30), (44, 77, 0, 40)"
            .to_string(),
        format!(
            "INSERT INTO soroban_contracts (id, contract_id, is_sac) VALUES \
             (77, '{SHARE_TOKEN}', false), (43, '{GAUGE}', false)"
        ),
        format!(
            "INSERT INTO soroban_contract_metadata (contract_id, decimals, version) VALUES ('{SHARE_TOKEN}', 7, 1)"
        ),
        format!(
            "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) VALUES \
             (42, '{ACCOUNT}', 1, 1, 1)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let first = fetch_soroban_participants(&ch, SOROBAN_POOL, None, 1, Direction::Next)
        .await
        .expect("participants query runs")
        .expect("a pool with a share token is indexed");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].account, ACCOUNT);
    assert_eq!(
        first[0].shares, "300",
        "newest balance, scaled by 7 decimals"
    );
    assert_eq!(first[0].share_percentage.as_deref(), Some("75"));
    assert_eq!(first[0].first_deposit_ledger, None);

    // The next page keys on the raw amount and still divides by the whole.
    let cursor = SharesCursor {
        shares: first[0].cursor_shares.clone(),
        account_id: first[0].account_id_surrogate,
    };
    let second = fetch_soroban_participants(&ch, SOROBAN_POOL, Some(&cursor), 10, Direction::Next)
        .await
        .expect("participants query runs")
        .expect("indexed");
    assert_eq!(second.len(), 1, "the zero balance is not a provider");
    assert_eq!(second[0].account, GAUGE, "a contract holder resolves");
    assert_eq!(second[0].share_percentage.as_deref(), Some("25"));

    assert!(
        fetch_soroban_participants(&ch, NO_TOKEN_POOL, None, 10, Direction::Next)
            .await
            .expect("participants query runs")
            .is_none(),
        "a pool without a share token is not indexed"
    );
    assert_eq!(
        fetch_soroban_participants(&ch, EMPTY_POOL, None, 10, Direction::Next)
            .await
            .expect("participants query runs")
            .map(|rows| rows.len()),
        Some(0),
        "a pool storing 0 total shares truly has no providers"
    );
    // The chain is the source of truth even where it disagrees with itself:
    // a pool whose stored total no holder backs lists nobody, and one whose
    // holders fall short of it divides by the pool's own total.
    assert_eq!(
        fetch_soroban_participants(&ch, UNREAD_POOL, None, 10, Direction::Next)
            .await
            .expect("participants query runs")
            .map(|rows| rows.len()),
        Some(0)
    );
    assert_eq!(
        count_soroban_participants(&ch, UNREAD_POOL)
            .await
            .expect("count runs"),
        Some(0)
    );
    let short = fetch_soroban_participants(&ch, PARTIAL_POOL, None, 10, Direction::Next)
        .await
        .expect("participants query runs")
        .expect("indexed");
    assert_eq!(
        short
            .iter()
            .map(|r| r.share_percentage.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("33.3333333"), Some("11.1111111")],
        "300 and 100 of the 900 shares the pool stores"
    );
    assert_eq!(
        count_soroban_participants(&ch, PARTIAL_POOL)
            .await
            .expect("count runs"),
        Some(2)
    );
    assert_eq!(
        count_soroban_participants(&ch, SOROBAN_POOL)
            .await
            .expect("count runs"),
        Some(2),
        "the detail count matches the list"
    );
    assert_eq!(
        count_soroban_participants(&ch, NO_TOKEN_POOL)
            .await
            .expect("count runs"),
        None
    );

    base.query(&format!("DROP DATABASE IF EXISTS {SOROBAN_DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
