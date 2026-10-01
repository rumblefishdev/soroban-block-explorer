//! ClickHouse-backed check of a soroban pool's activity (task 0374): the real
//! `init.sql`, the real query, a throwaway database. Gated on `CH_URL` like
//! the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_pool_activity

use crate::common::cursor::Direction;
use crate::liquidity_pools::dto::{PoolActivityCursor, PoolEvent};
use crate::liquidity_pools::queries::{fetch_pool_asset_ids, fetch_soroban_pool_activity};

const DB: &str = "api_test_0374_soroban_activity";

const POOL: &str = "5656565656565656565656565656565656565656565656565656565656565656";
const ROUTER_POOL: &str = "5757575757575757575757575757575757575757575757575757575757575757";
const TRADER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

/// Three operations, each in its own ledger span from the tip, so the read has
/// to widen its window twice to reach the oldest:
/// - 52,000,000: a trade, written twice (the live writer and the backfill);
/// - 51,200,000: a deposit naming two of the pool's three legs;
/// - 50,000,100: a withdrawal of all three.
///
/// And in a second pool, one operation that emitted 150 trade events, which
/// pages must split mid-operation by `event_index`.
#[tokio::test]
async fn soroban_pool_activity() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban activity check");
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
    // Keep the duplicate trade as an unmerged twin, as production has them.
    ch.query("SYSTEM STOP MERGES pool_movements")
        .execute()
        .await
        .expect("stop merges");

    let trade = format!(
        "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
         (unhex('{POOL}'), 52000000, 1, 0, 3, 0, 101, 500), \
         (unhex('{POOL}'), 52000000, 1, 0, 3, 0, 102, -499)"
    );
    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POOL}'), 30, 100, 1, [101, 102, 103]), \
             (unhex('{ROUTER_POOL}'), 30, 100, 1, [101, 102])"
        ),
        format!(
            "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) VALUES \
             (7, '{TRADER}', 1, 1, 1)"
        ),
        "INSERT INTO ledgers (sequence, closed_at) VALUES \
         (50000100, '2024-03-01 00:00:00'), (51200000, '2024-05-01 00:00:00'), \
         (52000000, '2024-07-01 00:00:00')"
            .to_string(),
        "INSERT INTO transactions (hash, ledger_sequence, application_order, source_id) VALUES \
         (unhex(repeat('aa', 32)), 50000100, 1, 7), \
         (unhex(repeat('bb', 32)), 51200000, 2, 7), \
         (unhex(repeat('cc', 32)), 52000000, 1, 7), \
         (unhex(repeat('dd', 32)), 52000000, 3, 7)"
            .to_string(),
        trade.clone(),
        // The same rows again, in a part of their own.
        trade,
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
             (unhex('{POOL}'), 51200000, 2, 1, 0, 1, 101, 7), \
             (unhex('{POOL}'), 51200000, 2, 1, 0, 1, 102, 123456789012345678901234567890), \
             (unhex('{POOL}'), 50000100, 1, 0, 0, 2, 101, -1), \
             (unhex('{POOL}'), 50000100, 1, 0, 0, 2, 102, -2), \
             (unhex('{POOL}'), 50000100, 1, 0, 0, 2, 103, -3)"
        ),
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) \
             SELECT unhex('{ROUTER_POOL}'), 52000000, 3, 0, intDiv(number, 2), 0, \
                    if(number % 2 = 0, 101, 102), if(number % 2 = 0, 10, -9) \
             FROM numbers(300)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let (kind, legs) = fetch_pool_asset_ids(&ch, POOL)
        .await
        .expect("legs query runs")
        .expect("pool exists");
    assert_eq!(kind, domain::PoolKind::Soroban);

    let page = |limit, cursor: Option<PoolActivityCursor>, direction, event| {
        let (ch, legs) = (ch.clone(), legs.clone());
        async move {
            fetch_soroban_pool_activity(&ch, POOL, &legs, limit, cursor.as_ref(), direction, event)
                .await
                .expect("activity query runs")
        }
    };
    let s = |v: &str| Some(v.to_string());

    // Newest first, all three, each once.
    let all = page(10, None, Direction::Next, None).await;
    let ledgers: Vec<i64> = all.iter().map(|r| r.ledger_sequence).collect();
    assert_eq!(ledgers, vec![52_000_000, 51_200_000, 50_000_100]);

    // The duplicated trade is counted once.
    assert_eq!(all[0].event, Some(PoolEvent::Trade));
    // A swap names the two tokens it moved; the third leg is not a zero.
    assert_eq!(all[0].amounts, vec![s("500"), s("-499"), None]);
    assert_eq!(all[0].event_index, Some(3));
    assert_eq!(all[0].transaction_hash, "cc".repeat(32));
    assert_eq!(all[0].source_account, TRADER);
    // A soroban route is not in `transaction_operations`: unknown, not 0.
    assert_eq!(all[0].pools_crossed, None);

    // The leg the event did not name stays `None` — never `0` — and the row
    // keeps its stored event; an amount past i64 stays exact.
    assert_eq!(all[1].event, Some(PoolEvent::Deposit));
    assert_eq!(
        all[1].amounts,
        vec![s("7"), s("123456789012345678901234567890"), None]
    );
    assert_eq!(all[2].event, Some(PoolEvent::Withdrawal));

    // The next page resumes after the cursor, across the widening windows.
    let after_trade = PoolActivityCursor {
        ledger_sequence: 52_000_000,
        application_order: 1,
        operation_index: 0,
        event_index: 3,
    };
    let older = page(10, Some(after_trade), Direction::Next, None).await;
    let ledgers: Vec<i64> = older.iter().map(|r| r.ledger_sequence).collect();
    assert_eq!(ledgers, vec![51_200_000, 50_000_100]);

    // Back from the oldest: ascending, as the pagination layer expects.
    let before_withdrawal = PoolActivityCursor {
        ledger_sequence: 50_000_100,
        application_order: 1,
        operation_index: 0,
        event_index: 0,
    };
    let newer = page(10, Some(before_withdrawal), Direction::Prev, None).await;
    let ledgers: Vec<i64> = newer.iter().map(|r| r.ledger_sequence).collect();
    assert_eq!(ledgers, vec![51_200_000, 52_000_000]);

    // The filter keeps only its event.
    let withdrawals = page(10, None, Direction::Next, Some(PoolEvent::Withdrawal)).await;
    let ledgers: Vec<i64> = withdrawals.iter().map(|r| r.ledger_sequence).collect();
    assert_eq!(ledgers, vec![50_000_100]);

    // A short page stops at its limit.
    let one = page(1, None, Direction::Next, None).await;
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].ledger_sequence, 52_000_000);

    // One operation, 150 events: two pages of 100 and 50, newest event
    // first, none repeated and none lost across the page edge.
    let (_, router_legs) = fetch_pool_asset_ids(&ch, ROUTER_POOL)
        .await
        .expect("legs query runs")
        .expect("router pool exists");
    let router_page = |cursor: Option<PoolActivityCursor>| {
        let (ch, legs) = (ch.clone(), router_legs.clone());
        async move {
            fetch_soroban_pool_activity(
                &ch,
                ROUTER_POOL,
                &legs,
                100,
                cursor.as_ref(),
                Direction::Next,
                None,
            )
            .await
            .expect("activity query runs")
        }
    };
    let first = router_page(None).await;
    let last = first.last().expect("a full first page");
    let second = router_page(Some(PoolActivityCursor {
        ledger_sequence: last.ledger_sequence,
        application_order: last.application_order,
        operation_index: last.operation_index,
        event_index: last.event_index.expect("a soroban row has an event"),
    }))
    .await;
    let events: Vec<u32> = first
        .iter()
        .chain(&second)
        .filter_map(|r| r.event_index)
        .collect();
    assert_eq!(events, (0..150).rev().collect::<Vec<u32>>());
    assert_eq!(first[0].amounts, vec![s("10"), s("-9")]);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
