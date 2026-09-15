//! `claimable_balance_holdings` through the production write path (task 0210).
//!
//! Unit tests stop at staging, and a row that is staged but never drained by
//! the writer is lost silently (the bug class the task 0374 e2e caught). This
//! drives `persist_ledger_clickhouse` against a live ClickHouse: a balance is
//! created in one ledger and claimed in a later one, and the table must end on
//! a single tombstone.
//!
//! Gated on `CLICKHOUSE_URL`. Run locally against a throwaway database:
//!
//!   CLICKHOUSE_URL=http://localhost:8123 CLICKHOUSE_DATABASE=scratch_0210 \
//!       cargo test -p db-clickhouse --test claimable_balance_holdings_e2e

use db_clickhouse::persist::{ClassificationCache, ids, persist_ledger_clickhouse};
use db_clickhouse::{Config, apply_init_sql, client};
use xdr_parser::claimable_balance::{ClaimableBalanceAsset, ExtractedClaimableBalance};
use xdr_parser::types::ExtractedLedger;

const CREATED_AT: u32 = 99_999_210;
const CLAIMED_AT: u32 = 99_999_215;
const BALANCE: &str = "BAAG4GHMU6GJVRBQLHS4LWHZPCOBJ6HYCUWBVZNQ7TPILPSGD6DQVTCBEM";
const ISSUER: &str = "GDKHHVS4SBVOLDGZNF2CVYW3TM7LRHK3NCVZE22MUEOYMQSMDQD2AVLX";

fn ledger(sequence: u32) -> ExtractedLedger {
    ExtractedLedger {
        sequence,
        hash: format!("{sequence:064x}"),
        closed_at: 1_789_000_000,
        protocol_version: 28,
        transaction_count: 0,
        base_fee: 100,
    }
}

fn holding(amount: i64, ledger_sequence: u32, closed: bool) -> ExtractedClaimableBalance {
    ExtractedClaimableBalance {
        balance_id: BALANCE.to_string(),
        asset: ClaimableBalanceAsset::Credit {
            code: "AVLX".to_string(),
            issuer: ISSUER.to_string(),
        },
        amount,
        ledger_sequence,
        closed,
    }
}

async fn persist(cl: &clickhouse::Client, sequence: u32, balances: &[ExtractedClaimableBalance]) {
    persist_ledger_clickhouse(
        cl,
        &ledger(sequence),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        balances,
        &[],
        &[],
        &[],
        &ClassificationCache::new(),
    )
    .await
    .expect("persist must succeed against live CH");
}

#[derive(clickhouse::Row, serde::Deserialize, Debug, PartialEq)]
struct Holding {
    asset_id: i64,
    amount: i128,
    last_updated_ledger: i64,
    closed_at_ledger: i64,
}

#[tokio::test]
async fn claim_supersedes_the_live_row() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping claimable balance e2e test");
        return;
    };
    let cl = client(&Config {
        url,
        ..Config::from_env()
    });
    apply_init_sql(&cl).await.expect("apply init.sql");
    let holder = ids::address_id(BALANCE);
    cl.query("ALTER TABLE claimable_balance_holdings DELETE WHERE holder_id = ? SETTINGS mutations_sync = 2")
        .bind(holder)
        .execute()
        .await
        .expect("clear leftovers");

    let read = || async {
        cl.query(
            "SELECT asset_id, amount, last_updated_ledger, closed_at_ledger \
             FROM claimable_balance_holdings FINAL WHERE holder_id = ?",
        )
        .bind(holder)
        .fetch_all::<Holding>()
        .await
        .expect("read holdings")
    };
    let asset_id = ids::credit_asset_id("AVLX", ISSUER);

    persist(&cl, CREATED_AT, &[holding(91_000, CREATED_AT, false)]).await;
    assert_eq!(
        read().await,
        vec![Holding {
            asset_id,
            amount: 91_000,
            last_updated_ledger: i64::from(CREATED_AT),
            closed_at_ledger: 0,
        }]
    );

    persist(&cl, CLAIMED_AT, &[holding(0, CLAIMED_AT, true)]).await;
    assert_eq!(
        read().await,
        vec![Holding {
            asset_id,
            amount: 0,
            last_updated_ledger: i64::from(CLAIMED_AT),
            closed_at_ledger: i64::from(CLAIMED_AT),
        }]
    );
}
