//! ClickHouse-backed check that the account's NFT count is the number of NFTs
//! it holds now (task 0675).
//!
//! `nfts` keeps one row per token, replaced on every ownership change, and on
//! production a replaced row can sit unmerged next to its successor. Merges are
//! stopped here so both rows stay on disk: counted on the stale row, the token
//! Alice sold to Bob would still count for Alice.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api nft_count

use super::*;

const DB: &str = "api_test_0675_nft_count";

#[tokio::test]
async fn an_account_counts_only_the_nfts_it_holds_now() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping NFT count check");
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

    for sql in [
        "SYSTEM STOP MERGES nfts",
        // Alice (10) holds two; she sells `sold` to Bob (11) in a later batch.
        "INSERT INTO nfts (contract_id, token_id, current_owner_id, current_owner_ledger) VALUES \
         (1, 'kept', 10, 100), (1, 'sold', 10, 100)",
        "INSERT INTO nfts (contract_id, token_id, current_owner_id, current_owner_ledger) VALUES \
         (1, 'sold', 11, 101)",
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }

    assert_eq!(fetch_nft_count(&ch, 10).await.expect("count Alice"), 1);
    assert_eq!(fetch_nft_count(&ch, 11).await.expect("count Bob"), 1);
    assert_eq!(fetch_nft_count(&ch, 12).await.expect("count Carol"), 0);

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
