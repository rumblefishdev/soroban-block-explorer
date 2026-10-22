//! ClickHouse-backed checks of the `nft_enrichment` write surface (task 0655):
//! the metadata JSON and the raw `token_uri` are stored, and the collection-name
//! stamp keeps them. Runs against the real `init.sql` in a throwaway database.
//! `#[ignore]`:
//!   CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
//!   CLICKHOUSE_PASSWORD=clickhouse cargo test -p enrichment-shared persist -- --ignored

use clickhouse::Row;
use serde::Deserialize;

use super::*;

const DB: &str = "enrichment_test_0655";

#[derive(Row, Deserialize, Debug, PartialEq)]
struct Stored {
    name: Option<String>,
    collection_name: Option<String>,
    metadata: String,
    token_uri: String,
}

async fn read(client: &Client, key: &NftKey) -> Stored {
    client
        .query(
            "SELECT name, collection_name, metadata, token_uri FROM nft_enrichment FINAL \
             WHERE contract_id = ? AND token_id = ?",
        )
        .bind(key.contract_id)
        .bind(&key.token_id)
        .fetch_one::<Stored>()
        .await
        .expect("read nft_enrichment")
}

#[tokio::test]
#[ignore = "needs live local ClickHouse"]
async fn metadata_and_token_uri_are_stored_and_survive_the_collection_stamp() {
    let base = db_clickhouse::client(&db_clickhouse::Config::from_env());
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let client = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&client)
        .await
        .expect("apply init.sql");

    let key = NftKey {
        contract_id: 7,
        token_id: "0".into(),
    };
    let metadata = r#"{"name":"Stroopies 0","attributes":[]}"#;
    let token_uri = r#"data:application/json,{"name":"Stroopies 0","attributes":[]}"#;
    let outcome = insert_nft(
        &client,
        &key,
        "Stroopies 0".into(),
        String::new(),
        String::new(),
        metadata.into(),
        token_uri.into(),
    )
    .await
    .expect("insert");
    assert_eq!(outcome, EnrichOutcome::Real);
    assert_eq!(read(&client, &key).await.metadata, metadata);

    // The stamp re-inserts the whole row; metadata and token_uri must come along.
    rewrite_nft_collection_name(&client, key.contract_id, "Stroopies")
        .await
        .expect("stamp");
    assert_eq!(
        read(&client, &key).await,
        Stored {
            name: Some("Stroopies 0".into()),
            collection_name: Some("Stroopies".into()),
            metadata: metadata.into(),
            token_uri: token_uri.into(),
        }
    );

    base.query(&format!("DROP DATABASE {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
