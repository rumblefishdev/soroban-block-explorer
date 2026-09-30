//! ClickHouse-backed check that an NFT held by a contract is served with the
//! contract's `C…` address (task 0376, K2-5).
//!
//! The owner surrogate lives in one space for accounts and contracts, and the
//! queries resolve it against both. Before #548 they read `accounts` only, so
//! a contract owner came back `null` — and a query refactor that drops the
//! `soroban_contracts` half would bring that back with every other test green.
//!
//! Runs the real queries against the real `init.sql` in a throwaway database
//! created and dropped here, so it never touches shared data. Gated on
//! `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api nft_held_by_a_contract_is_served_with_its_address

use super::*;
use crate::common::cursor::Direction;

const DB: &str = "api_test_0376_contract_owner";

const COLLECTION: &str = "CCOLLECTION";
const VAULT: &str = "CVAULT";
const ALICE: &str = "GALICE";

async fn seed(ch: &clickhouse::Client) {
    for sql in [
        "INSERT INTO soroban_contracts \
         (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, \
          contract_type, is_sac, executable_owner_id, executable_tag) VALUES \
         (1, 'CCOLLECTION', NULL, 90, NULL, 90, 1, false, NULL, NULL), \
         (2, 'CVAULT',      NULL, 90, NULL, 90, 1, false, NULL, NULL)",
        "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) \
         VALUES (10, 'GALICE', 90, 101, 1)",
        "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) VALUES \
         (100, unhex(repeat('01', 32)), '2026-01-01 00:00:00', 23, 2, 100), \
         (101, unhex(repeat('02', 32)), '2026-01-01 00:00:05', 23, 1, 100)",
        // Alice mints both; one then moves into the vault contract.
        "INSERT INTO nfts (contract_id, token_id, current_owner_id, current_owner_ledger) VALUES \
         (1, 'held-by-contract', 2,  101), \
         (1, 'held-by-account',  10, 100)",
        "INSERT INTO nft_ownership_changes \
         (contract_id, token_id, ledger_sequence, application_order, operation_index, event_index, \
          owner_id, event_type) VALUES \
         (1, 'held-by-contract', 100, 1, 0, 0, 10, 0), \
         (1, 'held-by-contract', 101, 1, 0, 0, 2,  1), \
         (1, 'held-by-account',  100, 2, 0, 0, 10, 0)",
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }
}

#[tokio::test]
async fn nft_held_by_a_contract_is_served_with_its_address() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping NFT contract-owner check");
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
    seed(&ch).await;

    // ---- list ----
    let params = ResolvedListParams {
        limit: 10,
        cursor: None,
        filter_collection: None,
        filter_contract_id: Some(COLLECTION.to_string()),
        filter_name: None,
    };
    let list = fetch_list(&ch, &params, Direction::Next)
        .await
        .expect("list must run");
    let owner = |token: &str| {
        list.iter()
            .find(|r| r.token_id == token)
            .unwrap_or_else(|| panic!("{token} must be listed"))
            .owner_account
            .clone()
    };
    assert_eq!(owner("held-by-contract").as_deref(), Some(VAULT));
    assert_eq!(owner("held-by-account").as_deref(), Some(ALICE));

    // ---- detail ----
    let detail = fetch_by_composite(&ch, COLLECTION, "held-by-contract")
        .await
        .expect("detail must run")
        .expect("detail must find the token");
    assert_eq!(detail.owner_account.as_deref(), Some(VAULT));

    // ---- transfers: newest first; the move into the vault starts at Alice ----
    let transfers = fetch_transfers(
        &ch,
        COLLECTION,
        "held-by-contract",
        None,
        10,
        Direction::Next,
    )
    .await
    .expect("transfers must run");
    let moves: Vec<_> = transfers
        .iter()
        .map(|t| (t.from_account.as_deref(), t.to_account.as_deref()))
        .collect();
    assert_eq!(moves, [(Some(ALICE), Some(VAULT)), (None, Some(ALICE))]);

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
