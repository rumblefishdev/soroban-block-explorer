//! ClickHouse-backed check of the events page (task 0584). Gated on `CH_URL`.

use super::*;

/// A contract's events page shows only ledgers whose `ledgers` row has
/// landed (the writer writes it last), and an event whose transaction is
/// missing from a landed ledger fails the page instead of showing an empty
/// hash and 1970 (task 0584).
#[tokio::test]
async fn events_page_hides_unlanded_ledgers_and_fails_on_a_missing_transaction() {
    const DB_0584: &str = "api_test_0584_contract_events";
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping contract events check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB_0584}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB_0584}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB_0584);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    for sql in [
        "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) \
         VALUES (100, unhex(repeat('01', 32)), '2026-10-07 10:00:00', 23, 1, 100)",
        "INSERT INTO transactions (hash, ledger_sequence, application_order, source_id, fee_charged, \
          inner_tx_hash, successful, operation_count, has_soroban, parse_error) \
         VALUES (unhex(repeat('aa', 32)), 100, 1, 1, 100, NULL, true, 1, true, false)",
        // Ledger 100's event; one of ledger 101, past the tip, whose `ledgers`
        // row has not landed; and one of ledger 99, below the tip, that a
        // backfill is still writing. Neither has its transaction yet.
        "INSERT INTO soroban_events (contract_id, ledger_sequence, transaction_index, \
          operation_index, event_index, application_order, event_type, signature, topics_xdr, data_xdr) \
         VALUES (7, 100, 1, 0, 0, 1, 1, NULL, '[]', 'null'), \
                (7, 101, 2, 0, 0, 2, 1, NULL, '[]', 'null'), \
                (7, 99, 3, 0, 0, 3, 1, NULL, '[]', 'null')",
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }

    let page = fetch_events(&ch, 7, 10, None, Direction::Next)
        .await
        .expect("a page of landed ledgers reads");
    assert_eq!(
        page.len(),
        1,
        "ledgers 99 and 101 have not landed, so they are not shown"
    );
    assert_eq!(page[0].item.transaction_hash, "aa".repeat(32));

    ch.query(
        "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) \
         VALUES (101, unhex(repeat('02', 32)), '2026-10-07 10:00:05', 23, 1, 100)",
    )
    .execute()
    .await
    .expect("land ledger 101");
    assert!(
        fetch_events(&ch, 7, 10, None, Direction::Next)
            .await
            .is_err(),
        "a landed ledger's event without its transaction is a broken row"
    );

    base.query(&format!("DROP DATABASE {DB_0584}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
