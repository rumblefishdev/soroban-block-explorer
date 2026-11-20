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
            .owner
            .clone()
    };
    assert_eq!(owner("held-by-contract").as_deref(), Some(VAULT));
    assert_eq!(owner("held-by-account").as_deref(), Some(ALICE));

    // ---- detail ----
    let detail = fetch_by_composite(&ch, COLLECTION, "held-by-contract")
        .await
        .expect("detail must run")
        .expect("detail must find the token");
    assert_eq!(detail.owner.as_deref(), Some(VAULT));

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
        .map(|t| (t.from.as_deref(), t.to.as_deref()))
        .collect();
    assert_eq!(moves, [(Some(ALICE), Some(VAULT)), (None, Some(ALICE))]);

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}

/// `filter[owner]` lists only what an account holds NOW, exactly as the list
/// shows it (task 0675).
///
/// `fetch_held_by` reads without a JOIN, so it re-states the list's rules for
/// every field. This compares it, row for row, with `fetch_list` filtered to
/// the same owner, over data that exercises each rule: a newer enrichment
/// version wins, a contract's own name wins over the enrichment collection, a
/// token without a mint row sorts last, and the page cursor walks the same rows.
///
/// `nfts` keeps one row per token, replaced on every ownership change, and on
/// production a replaced row can sit unmerged next to its successor. Merges
/// are stopped here so both rows stay on disk: matched against the stale row,
/// the token Alice sold to Bob would still be listed under Alice.
#[tokio::test]
async fn nfts_held_by_an_owner_match_the_list_for_that_owner() {
    const DB: &str = "api_test_0675_owner_filter";
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping NFT owner-filter check");
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
        "INSERT INTO soroban_contracts \
         (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, \
          contract_type, is_sac, executable_owner_id, executable_tag) VALUES \
         (1, 'CNAMED',   NULL, 90, NULL, 90, 1, false, NULL, NULL), \
         (2, 'CUNNAMED', NULL, 90, NULL, 90, 1, false, NULL, NULL)",
        // Only CNAMED publishes a name of its own.
        "INSERT INTO soroban_contract_metadata (contract_id, name) VALUES ('CNAMED', 'Ledger Name')",
        "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) \
         VALUES (10, 'GALICE', 90, 101, 1), (11, 'GBOB', 90, 101, 1), (12, 'GCAROL', 90, 101, 1)",
        // Alice holds three; she sells `sold` to Bob in a later batch — a
        // second row for the same token, left unmerged.
        "INSERT INTO nfts (contract_id, token_id, current_owner_id, current_owner_ledger) VALUES \
         (1, 'kept', 10, 100), (1, 'sold', 10, 100), (2, 'unminted', 10, 99), (2, 'plain', 10, 98)",
        "INSERT INTO nfts (contract_id, token_id, current_owner_id, current_owner_ledger) VALUES \
         (1, 'sold', 11, 101)",
        // `kept` has two enrichment versions: the newer one must win.
        "INSERT INTO nft_enrichment (contract_id, token_id, name, media_url, collection_name, version) VALUES \
         (1, 'kept', 'Old Name', 'https://old', 'Old Coll', '2026-01-01 00:00:00'), \
         (1, 'kept', 'Kept #1', 'https://kept', 'Enriched Coll', '2026-01-02 00:00:00'), \
         (2, 'plain', 'Plain', NULL, 'Enriched Only', '2026-01-02 00:00:00')",
        // `unminted` has no mint row, so it has no mint ledger and sorts last.
        "INSERT INTO nft_ownership_changes \
         (contract_id, token_id, ledger_sequence, application_order, operation_index, event_index, \
          owner_id, event_type) VALUES \
         (1, 'kept', 100, 1, 0, 0, 10, 0), \
         (1, 'sold', 100, 2, 0, 0, 10, 0), \
         (1, 'sold', 101, 1, 0, 0, 11, 1), \
         (2, 'plain', 98, 1, 0, 0, 10, 0)",
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }

    let shown = |r: &NftRow| {
        (
            r.contract_id.clone(),
            r.token_id.clone(),
            r.collection_name.clone(),
            r.name.clone(),
            r.media_url.clone(),
            r.minted_at_ledger,
            r.owner.clone(),
            r.last_seen_ledger,
        )
    };
    let everything = ResolvedListParams {
        limit: 100,
        cursor: None,
        filter_collection: None,
        filter_contract_id: None,
        filter_name: None,
    };
    let list = fetch_list(&ch, &everything, Direction::Next)
        .await
        .expect("list must run");

    for owner in ["GALICE", "GBOB", "GCAROL", "GNOBODY"] {
        let expected: Vec<_> = list
            .iter()
            .filter(|r| r.owner.as_deref() == Some(owner))
            .map(shown)
            .collect();
        let held: Vec<_> = fetch_held_by(&ch, owner, 100, None, Direction::Next)
            .await
            .expect("held-by must run")
            .iter()
            .map(shown)
            .collect();
        assert_eq!(held, expected, "{owner}");
    }

    // The rules the comparison rests on, stated once on Alice's rows.
    let alice = fetch_held_by(&ch, "GALICE", 100, None, Direction::Next)
        .await
        .expect("held-by must run");
    let tokens: Vec<_> = alice.iter().map(|r| r.token_id.as_str()).collect();
    assert_eq!(tokens, ["kept", "plain", "unminted"]);
    assert_eq!(alice[0].name.as_deref(), Some("Kept #1"));
    assert_eq!(alice[0].collection_name.as_deref(), Some("Ledger Name"));
    assert_eq!(alice[1].collection_name.as_deref(), Some("Enriched Only"));
    assert_eq!(alice[2].minted_at_ledger, None);

    // One row per page: the cursor walks the same rows in the same order.
    let mut walked = Vec::new();
    let mut cursor: Option<NftListCursor> = None;
    loop {
        // `limit` 2 = one row plus the handler's look-ahead row.
        let page = fetch_held_by(&ch, "GALICE", 2, cursor.as_ref(), Direction::Next)
            .await
            .expect("page must run");
        let Some(row) = page.first() else { break };
        walked.push(row.token_id.clone());
        if page.len() < 2 {
            break;
        }
        cursor = Some(NftListCursor {
            minted_at_ledger: row.minted_at_ledger.unwrap_or(0),
            contract_surrogate: row.contract_surrogate,
            token_id: row.token_id.clone(),
        });
    }
    assert_eq!(walked, ["kept", "plain", "unminted"]);

    // And back from the last row: each Prev page starts at the row just before.
    let mut walked_back = Vec::new();
    let last = alice.last().expect("Alice holds NFTs");
    let mut cursor = NftListCursor {
        minted_at_ledger: last.minted_at_ledger.unwrap_or(0),
        contract_surrogate: last.contract_surrogate,
        token_id: last.token_id.clone(),
    };
    loop {
        let page = fetch_held_by(&ch, "GALICE", 2, Some(&cursor), Direction::Prev)
            .await
            .expect("page must run");
        let Some(row) = page.first() else { break };
        walked_back.push(row.token_id.clone());
        cursor = NftListCursor {
            minted_at_ledger: row.minted_at_ledger.unwrap_or(0),
            contract_surrogate: row.contract_surrogate,
            token_id: row.token_id.clone(),
        };
    }
    assert_eq!(walked_back, ["plain", "kept"]);

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
