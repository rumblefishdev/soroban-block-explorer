//! ClickHouse-backed check that the asset detail page's SEP-1 text is read from
//! what the enrichment worker stored (task 0655), in a throwaway database built
//! from the real `init.sql`. Gated on `CH_URL` like the other DB-backed tests.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api sep1_detail_text

use super::*;

const DB: &str = "api_test_0655_sep1_detail_text";

#[tokio::test]
async fn detail_text_is_the_stored_sep1_read() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping SEP-1 detail text check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    for sql in [
        "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) \
         VALUES (10, 'GCIRCLE', 1, 1, 1), (11, 'GQUIET', 1, 1, 1)",
        // An older read, then the latest one: the latest wins.
        "INSERT INTO asset_enrichment \
         (asset_type, asset_code, issuer_id, contract_id, icon_url, name, version, description, home_page) VALUES \
         (1, 'USDC', 10, 0, '', 'USD Coin', '2026-01-01 00:00:00', 'old', 'https://old.example'), \
         (1, 'USDC', 10, 0, '', 'USD Coin', '2026-01-02 00:00:00', 'A fully reserved stablecoin', 'https://www.circle.com'), \
         (1, 'QUIET', 11, 0, '', '', '2026-01-01 00:00:00', 'removed since', 'https://gone.example'), \
         (1, 'QUIET', 11, 0, '', '', '2026-01-02 00:00:00', NULL, NULL)",
    ] {
        ch.query(sql).execute().await.expect("seed row");
    }

    assert_eq!(
        fetch(&ch, "USDC", "GCIRCLE").await.expect("query runs"),
        (
            Some("A fully reserved stablecoin".to_owned()),
            Some("https://www.circle.com".to_owned())
        )
    );
    assert_eq!(
        fetch(&ch, "QUIET", "GQUIET").await.expect("query runs"),
        (None, None),
        "the latest read had none → null, even though an older read had text"
    );
    assert_eq!(
        fetch(&ch, "NONE", "GNOBODY").await.expect("query runs"),
        (None, None),
        "unknown issuer → null"
    );

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
