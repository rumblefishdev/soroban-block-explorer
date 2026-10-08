use super::*;
use serde_json::json;

#[test]
fn extract_columns_pulls_standard_fields() {
    let blob = json!({
        "name": "Punk #4521",
        "image": "https://example.com/4521.png",
        "collection": "CryptoPunks",
        "attributes": [{"trait_type": "Hat", "value": "Beanie"}]
    });
    let (name, image, collection) = extract_columns(&blob);
    assert_eq!(name, "Punk #4521");
    assert_eq!(image, "https://example.com/4521.png");
    assert_eq!(collection, "CryptoPunks");
}

#[test]
fn extract_columns_returns_empty_for_missing_keys() {
    let blob = json!({});
    assert_eq!(
        extract_columns(&blob),
        (String::new(), String::new(), String::new())
    );
}

#[test]
fn extract_columns_trims_whitespace() {
    let blob = json!({"name": "  Spaced  ", "image": "", "collection": null});
    let (name, image, collection) = extract_columns(&blob);
    assert_eq!(name, "Spaced");
    assert_eq!(image, "");
    assert_eq!(collection, "");
}

#[test]
fn trimmed_string_chars_caps_oversize_to_sentinel() {
    let too_long = "x".repeat(MAX_NAME_CHARS + 1);
    let v = Value::String(too_long);
    assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), "");
}

#[test]
fn trimmed_string_chars_uses_char_count_not_byte_length() {
    // `MAX_NAME_CHARS` multi-byte chars (each emoji = 4 bytes) → 4× bytes but
    // exactly the char cap. Char-cap MUST accept this; a byte-cap would have
    // wrongly rejected it.
    let exactly_max = "🚀".repeat(MAX_NAME_CHARS);
    assert_eq!(exactly_max.chars().count(), MAX_NAME_CHARS);
    assert!(exactly_max.len() > MAX_NAME_CHARS); // confirm bytes > chars
    let v = Value::String(exactly_max.clone());
    assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), exactly_max);

    // One char over the cap → sentinel.
    let over = "🚀".repeat(MAX_NAME_CHARS + 1);
    let v = Value::String(over);
    assert_eq!(trimmed_string_chars(Some(&v), MAX_NAME_CHARS), "");
}

#[test]
fn trimmed_string_chars_handles_non_string() {
    assert_eq!(trimmed_string_chars(Some(&json!(42)), 256), "");
    assert_eq!(trimmed_string_chars(Some(&json!(null)), 256), "");
    assert_eq!(trimmed_string_chars(None, 256), "");
}

#[test]
fn trimmed_string_bytes_caps_for_text_columns() {
    let too_long = "x".repeat(MAX_MEDIA_URL_BYTES + 1);
    let v = Value::String(too_long);
    assert_eq!(trimmed_string_bytes(Some(&v), MAX_MEDIA_URL_BYTES), "");
}

#[test]
fn extract_columns_resolves_ipfs_image_to_https() {
    let blob = json!({
        "name": "Punk #1",
        "image": "ipfs://QmFoo/1.png",
        "collection": "X"
    });
    let (_, image, _) = extract_columns(&blob);
    assert!(
        image.starts_with("https://"),
        "ipfs:// must be resolved, got {image}"
    );
    assert!(image.ends_with("QmFoo/1.png"));
}

#[test]
fn extract_columns_replaces_unsafe_image_with_sentinel() {
    let blob = json!({
        "name": "Punk",
        "image": "javascript:alert(1)",
        "collection": "X"
    });
    let (name, image, collection) = extract_columns(&blob);
    assert_eq!(name, "Punk");
    assert_eq!(image, ""); // sentinel — not the malicious scheme
    assert_eq!(collection, "X");
}

/// CH-backed end-to-end smoke (task 0231 step 5): the full nft write path
/// against a live local ClickHouse — `soroban_contracts` StrKey lookup →
/// live `token_uri()` RPC (+ metadata fetch) → INSERT → read back. Covers
/// the **real** path (a known mainnet contract) and the **sentinel** path
/// (contract absent from `soroban_contracts`). `#[ignore]` (needs CH +
/// mainnet Soroban-RPC + reachable token_uri metadata). Run:
/// `CLICKHOUSE_URL=http://localhost:8125 CLICKHOUSE_USER=default \
///  CLICKHOUSE_PASSWORD=clickhouse cargo test -p enrichment-shared \
///  smoke_ch_nft -- --ignored --nocapture`
#[tokio::test]
#[ignore = "needs live local ClickHouse + mainnet Soroban-RPC + reachable token_uri metadata"]
async fn smoke_ch_nft_real_and_sentinel() {
    #[derive(Row, Deserialize)]
    struct Readback {
        name: Option<String>,
        media_url: Option<String>,
        collection_name: Option<String>,
    }
    async fn read(client: &Client, k: &NftKey) -> (Option<String>, Option<String>, Option<String>) {
        let r = client
            .query(
                "SELECT name, media_url, collection_name FROM nft_enrichment FINAL \
                 WHERE contract_id = ? AND token_id = ?",
            )
            .bind(k.contract_id)
            .bind(&k.token_id)
            .fetch_one::<Readback>()
            .await
            .expect("read nft_enrichment");
        (r.name, r.media_url, r.collection_name)
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    let fetcher = NftTokenUriFetcher::with_rpc_url("https://mainnet.sorobanrpc.com".to_owned())
        .expect("build fetcher");

    // --- REAL: seed a known mainnet NFT contract (0-arg token_uri; the
    // fetcher's arity fallback handles it) ---
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
        .expect("seed soroban contract");

    let key = NftKey {
        contract_id,
        token_id: "1".into(),
    };
    enrich_nft_token_uri(&client, key.clone(), &fetcher)
        .await
        .expect("enrich nft");
    let (name, media, coll) = read(&client, &key).await;
    eprintln!("REAL NFT -> name={name:?} media={media:?} coll={coll:?}");
    assert!(
        [&name, &media, &coll]
            .iter()
            .any(|c| c.as_deref().is_some_and(|s| !s.is_empty())),
        "real path: at least one non-empty metadata field \
         (depends on the contract's token_uri target staying reachable)"
    );

    // --- SENTINEL: a contract absent from `soroban_contracts` ---
    let ghost = NftKey {
        contract_id: 808_001,
        token_id: "1".into(),
    };
    enrich_nft_token_uri(&client, ghost.clone(), &fetcher)
        .await
        .expect("enrich ghost nft");
    let (gn, gm, gc) = read(&client, &ghost).await;
    assert_eq!(
        (gn.as_deref(), gm.as_deref(), gc.as_deref()),
        (Some(""), Some(""), Some("")),
        "sentinel: all columns = ''"
    );

    // --- cleanup (dev CH is otherwise empty) ---
    client
        .query("ALTER TABLE soroban_contracts DELETE WHERE id = ?")
        .bind(contract_id)
        .execute()
        .await
        .expect("cleanup soroban_contracts");
    client
        .query("ALTER TABLE nft_enrichment DELETE WHERE token_id = '1'")
        .execute()
        .await
        .expect("cleanup nft_enrichment");
}
