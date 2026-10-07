//! Unit tests cover the testable kernel of the worker:
//!   - `EnrichmentMessage` deserialisation (tagged enum contract)
//!   - `parse_message` body / JSON / kind error mapping
//!   - `classify_outcome` ack-vs-retry decision
//!   - `require_message_id` rejection of malformed records
//!
//! `handle_record` (DB + HTTP) and `handle_event` (full Lambda glue)
//! are not covered here — they require a live `clickhouse::Client` and `Sep1Fetcher`,
//! which are the responsibility of the per-kind tests in
//! `enrichment-shared` and a deploy-time smoke test.
use super::*;

fn record(message_id: Option<&str>, body: Option<&str>) -> SqsMessage {
    SqsMessage {
        message_id: message_id.map(str::to_owned),
        receipt_handle: None,
        body: body.map(str::to_owned),
        md5_of_body: None,
        md5_of_message_attributes: None,
        attributes: Default::default(),
        message_attributes: Default::default(),
        event_source_arn: None,
        event_source: None,
        aws_region: None,
    }
}

// -- EnrichmentMessage serde -------------------------------------

#[test]
fn enrichment_message_parses_sep1_assets_variant() {
    let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":42,"contract_id":7}"#;
    let msg: EnrichmentMessage = serde_json::from_str(json).expect("parse");
    let EnrichmentMessage::Sep1Assets(key) = msg else {
        panic!("expected Sep1Assets variant, got {msg:?}");
    };
    assert_eq!(key.asset_type, 1);
    assert_eq!(key.asset_code, "USDC");
    assert_eq!(key.issuer_id, 42);
    assert_eq!(key.contract_id, 7);
}

#[test]
fn enrichment_message_parses_nft_token_uri_variant() {
    let json = r#"{"kind":"nft_token_uri","contract_id":99,"token_id":"3"}"#;
    let msg: EnrichmentMessage = serde_json::from_str(json).expect("parse");
    let EnrichmentMessage::NftTokenUri(key) = msg else {
        panic!("expected NftTokenUri variant, got {msg:?}");
    };
    assert_eq!(key.contract_id, 99);
    assert_eq!(key.token_id, "3");
}

#[test]
fn enrichment_message_rejects_unknown_kind() {
    // Future-kind safety — adding `lp_tvl` later requires a code
    // change here, not a silent ack-and-drop on the worker side.
    let json = r#"{"kind":"lp_tvl","pool_id":1}"#;
    assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
}

#[test]
fn enrichment_message_rejects_missing_kind() {
    let json = r#"{"asset_type":1,"asset_code":"USDC","issuer_id":42,"contract_id":7}"#;
    assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
}

#[test]
fn enrichment_message_rejects_missing_key_field() {
    // contract_id absent — a producer that drops a key field is a bug.
    let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":42}"#;
    assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
}

#[test]
fn enrichment_message_rejects_wrong_key_type() {
    // issuer_id is i64 — a string is a producer bug.
    let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":"42","contract_id":7}"#;
    assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
}

// -- parse_message -----------------------------------------------

#[test]
fn parse_message_returns_permanent_on_missing_body() {
    let r = record(Some("m-1"), None);
    match parse_message(&r) {
        Err(RecordError::Permanent(msg)) => assert!(msg.contains("no body")),
        other => panic!("expected Permanent, got {other:?}"),
    }
}

#[test]
fn parse_message_returns_permanent_on_malformed_json() {
    let r = record(Some("m-1"), Some("{not json"));
    match parse_message(&r) {
        Err(RecordError::Permanent(msg)) => {
            assert!(msg.contains("malformed enrichment JSON"))
        }
        other => panic!("expected Permanent, got {other:?}"),
    }
}

#[test]
fn parse_message_returns_sep1_assets_on_well_formed_body() {
    let r = record(
        Some("m-1"),
        Some(
            r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":7,"contract_id":0}"#,
        ),
    );
    let msg = parse_message(&r).expect("ok");
    let EnrichmentMessage::Sep1Assets(key) = msg else {
        panic!("expected Sep1Assets variant, got {msg:?}");
    };
    assert_eq!(key.issuer_id, 7);
}

// -- classify_outcome --------------------------------------------

#[test]
fn classify_outcome_acks_ok() {
    assert!(classify_outcome("m-1", Ok(())).is_none());
}

#[test]
fn classify_outcome_acks_permanent_error() {
    let outcome = Err(RecordError::Permanent("bad json".to_owned()));
    assert!(classify_outcome("m-1", outcome).is_none());
}

#[test]
fn classify_outcome_emits_partial_failure_on_transient_error() {
    let outcome = Err(RecordError::Transient(EnrichError::Transient(
        "5xx from issuer".to_owned(),
    )));
    let failure = classify_outcome("m-42", outcome).expect("partial failure");
    assert_eq!(failure.item_identifier, "m-42");
}

#[test]
fn classify_outcome_emits_partial_failure_on_database_error() {
    // `Custom` is the cheapest ClickHouse error variant to construct;
    // the bucket assertion is what we care about, not the exact error.
    let outcome = Err(RecordError::Transient(EnrichError::Database(
        clickhouse::error::Error::Custom("pool timed out".to_owned()),
    )));
    let failure = classify_outcome("m-99", outcome).expect("partial failure");
    assert_eq!(failure.item_identifier, "m-99");
}

// -- require_message_id ------------------------------------------

#[test]
fn require_message_id_returns_id_when_present() {
    let r = record(Some("abc-123"), Some(""));
    assert_eq!(require_message_id(&r).expect("ok"), "abc-123");
}

#[test]
fn require_message_id_errors_when_missing() {
    let r = record(None, Some(""));
    assert!(require_message_id(&r).is_err());
}

/// CH-backed worker smoke (task 0231 step 5): the full per-message path —
/// SQS body → `parse_message` → `handle_record` dispatch → `enrich_*` → CH.
/// `#[ignore]` (needs live local CH + network). Run:
/// `CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
///  CLICKHOUSE_PASSWORD=clickhouse cargo test -p enrichment-worker \
///  handle_record_ -- --ignored --nocapture`
#[tokio::test]
#[ignore = "needs live local ClickHouse + network (centre.io TOML)"]
async fn handle_record_sep1_writes_real_enrichment() {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct R {
        name: Option<String>,
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    let state = WorkerState {
        client: client.clone(),
        sep1: Sep1Fetcher::new().expect("sep1"),
        nft_token_uri: NftTokenUriFetcher::new().expect("nft"),
    };
    let issuer = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let issuer_id = db_clickhouse::persist::ids::account_id(issuer);
    client
        .query(
            "INSERT INTO accounts \
             (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain) \
             VALUES (?, ?, 0, 0, 0, 'centre.io')",
        )
        .bind(issuer_id)
        .bind(issuer)
        .execute()
        .await
        .expect("seed issuer");

    let body = format!(
        r#"{{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":{issuer_id},"contract_id":0}}"#
    );
    handle_record(&record(Some("m-sep1"), Some(&body)), &state)
        .await
        .expect("handle_record");

    let r: R = client
        .query(
            "SELECT name FROM asset_enrichment FINAL WHERE asset_code = 'USDC' AND issuer_id = ?",
        )
        .bind(issuer_id)
        .fetch_one()
        .await
        .expect("readback");
    assert!(
        r.name.as_deref().is_some_and(|s| !s.is_empty()),
        "worker decode→dispatch→enrich wrote a real USDC name"
    );

    client
        .query("ALTER TABLE accounts DELETE WHERE id = ?")
        .bind(issuer_id)
        .execute()
        .await
        .expect("cleanup acc");
    client
        .query("ALTER TABLE asset_enrichment DELETE WHERE asset_code = 'USDC'")
        .execute()
        .await
        .expect("cleanup enr");
}

#[tokio::test]
#[ignore = "needs live local ClickHouse + mainnet Soroban-RPC"]
async fn handle_record_nft_writes_real_enrichment() {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct R {
        name: Option<String>,
        media_url: Option<String>,
        collection_name: Option<String>,
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    let state = WorkerState {
        client: client.clone(),
        sep1: Sep1Fetcher::new().expect("sep1"),
        nft_token_uri: NftTokenUriFetcher::new().expect("nft"),
    };
    let contract = "CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY";
    let contract_id = db_clickhouse::persist::ids::contract_id(contract);
    client
        .query(
            "INSERT INTO soroban_contracts (id, contract_id, wasm_uploaded_at_ledger, is_sac) \
             VALUES (?, ?, 0, false)",
        )
        .bind(contract_id)
        .bind(contract)
        .execute()
        .await
        .expect("seed contract");

    let body = format!(r#"{{"kind":"nft_token_uri","contract_id":{contract_id},"token_id":"1"}}"#);
    handle_record(&record(Some("m-nft"), Some(&body)), &state)
        .await
        .expect("handle_record");

    let r: R = client
        .query(
            "SELECT name, media_url, collection_name FROM nft_enrichment FINAL \
             WHERE contract_id = ? AND token_id = '1'",
        )
        .bind(contract_id)
        .fetch_one()
        .await
        .expect("readback");
    assert!(
        [&r.name, &r.media_url, &r.collection_name]
            .iter()
            .any(|c| c.as_deref().is_some_and(|s| !s.is_empty())),
        "worker decode→dispatch→enrich wrote a real NFT metadata field"
    );

    client
        .query("ALTER TABLE soroban_contracts DELETE WHERE id = ?")
        .bind(contract_id)
        .execute()
        .await
        .expect("cleanup contract");
    client
        .query("ALTER TABLE nft_enrichment DELETE WHERE token_id = '1'")
        .execute()
        .await
        .expect("cleanup enr");
}
