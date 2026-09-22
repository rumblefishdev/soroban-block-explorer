use super::*;
use aws_config::BehaviorVersion;

/// Build an unsigned S3 client for anonymous access to the public archive.
async fn unsigned_client() -> S3Client {
    let config = aws_config::defaults(BehaviorVersion::latest())
        .no_credentials()
        .region("us-east-2") // aws-public-blockchain is in us-east-2
        .timeout_config(default_timeout_config())
        .load()
        .await;
    S3Client::new(&config)
}

/// End-to-end fetch of a known Soroban-era ledger from the public archive.
///
/// Ignored by default — requires network access + writes no state. Run with:
///   `cargo test --package api -- --ignored runtime_enrichment::stellar_archive::tests::fetch_single_ledger_from_archive`
#[tokio::test]
#[ignore = "requires network access to aws-public-blockchain"]
async fn fetch_single_ledger_from_archive() {
    use stellar_xdr::LedgerCloseMeta;

    let fetcher = StellarArchiveFetcher::new(unsigned_client().await);
    // First Soroban-era ledger (backfill runbook).
    let seq = 50_457_424_u32;

    let meta = fetcher.fetch_ledger(seq).await.expect("fetch failed");

    let got_seq = match &meta {
        LedgerCloseMeta::V0(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(v) => v.ledger_header.header.ledger_seq,
    };
    assert_eq!(got_seq, seq, "ledger sequence mismatch");
}

/// Batch fetch — verifies concurrent fetch path.
#[tokio::test]
#[ignore = "requires network access to aws-public-blockchain"]
async fn fetch_multiple_ledgers_concurrently_from_stellar_archive() {
    let fetcher = StellarArchiveFetcher::new(unsigned_client().await);
    let seqs = [50_457_424_u32, 50_457_425, 50_457_426];

    let results = fetcher.fetch_ledgers(&seqs).await;

    assert_eq!(results.len(), seqs.len());
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "ledger {} failed: {:?}", seqs[i], result);
    }
}

/// Scan a sequence of ledgers for one containing at least one transaction
/// and return (ledger_meta, first_tx_hash). Gives the E3 end-to-end
/// tests a non-empty ledger without hard-coding a tx hash that may
/// disappear if the public archive ever changes format.
async fn find_ledger_with_tx(
    fetcher: &StellarArchiveFetcher,
    start: u32,
    window: u32,
) -> (LedgerCloseMeta, String) {
    for seq in start..start + window {
        let Ok(meta) = fetcher.fetch_ledger(seq).await else {
            continue;
        };
        let ledger = xdr_parser::extract_ledger(&meta);
        let net_id = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);
        let txs =
            xdr_parser::extract_transactions(&meta, ledger.sequence, ledger.closed_at, &net_id);
        if let Some(first_tx) = txs.iter().find(|t| !t.parse_error) {
            return (meta, first_tx.hash.clone());
        }
    }
    panic!(
        "no ledger with a parseable tx in range [{start}, {}]",
        start + window
    );
}

/// End-to-end E3 pipeline test:
/// fetch real ledger → extract heavy fields for a real transaction hash
/// → verify the heavy-field shape looks sensible.
#[tokio::test]
#[ignore = "requires network access to aws-public-blockchain"]
async fn extract_e3_heavy_fields_from_real_stellar_tx() {
    use super::extractors::extract_e3_heavy;

    let fetcher = StellarArchiveFetcher::new(unsigned_client().await);
    // Start at first Soroban ledger; scan up to 20 ledgers for one with txs.
    let (meta, tx_hash) = find_ledger_with_tx(&fetcher, 50_457_424, 20).await;

    let net_id = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);
    let heavy = extract_e3_heavy(&meta, &tx_hash, &net_id).expect("tx hash found in ledger");

    // XDR blobs must be populated for a non-parse-error tx.
    assert!(
        heavy.envelope_xdr.is_some(),
        "envelope_xdr missing: {tx_hash}"
    );
    assert!(heavy.result_xdr.is_some(), "result_xdr missing: {tx_hash}");
    // Signatures: at least one on a genuine Stellar tx.
    assert!(
        !heavy.signatures.is_empty(),
        "expected ≥1 signature on tx {tx_hash}"
    );
    // Every signature should be non-empty hex.
    for sig in &heavy.signatures {
        assert_eq!(sig.hint.len(), 8, "hint must be 4 bytes hex");
        assert!(!sig.signature.is_empty(), "empty signature bytes");
    }
}

/// Failed classic tx (task 0352 / issue #364 fixture `7af6d0ed…`, ledger
/// 63687496): the per-op result codes must survive to the DTO —
/// `BEGIN_SPONSORING_FUTURE_RESERVES` succeeded, `CREATE_ACCOUNT` failed
/// with `LowReserve`, the third op was rejected op-level (`OpNoAccount`).
#[tokio::test]
#[ignore = "requires network access to aws-public-blockchain"]
async fn e3_failed_tx_ops_carry_result_codes() {
    use super::extractors::extract_e3_heavy;

    let fetcher = StellarArchiveFetcher::new(unsigned_client().await);
    let meta = fetcher.fetch_ledger(63_687_496).await.unwrap();

    let tx_hash = "7af6d0edad166f2ec276fc75e13d0613c70d9476c164db943ce64e183a44f6c5";
    let net_id = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);
    let heavy = extract_e3_heavy(&meta, tx_hash, &net_id).expect("fixture tx in ledger");

    assert_eq!(heavy.result_code.as_deref(), Some("TxFailed"));
    let codes: Vec<_> = heavy
        .operations
        .iter()
        .map(|op| op.result_code.as_deref())
        .collect();
    assert_eq!(
        codes,
        vec![Some("Success"), Some("LowReserve"), Some("OpNoAccount")]
    );
}

/// E3 heavy extraction should return None for an unknown tx hash.
#[tokio::test]
#[ignore = "requires network access to aws-public-blockchain"]
async fn e3_extractor_return_none_for_unknown_tx_hash() {
    use super::extractors::extract_e3_heavy;

    let fetcher = StellarArchiveFetcher::new(unsigned_client().await);
    let meta = fetcher.fetch_ledger(50_457_424).await.unwrap();

    // Not a real tx hash in this ledger.
    let fake_hash = "deadbeef".repeat(8);
    let net_id = xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE);
    assert!(extract_e3_heavy(&meta, &fake_hash, &net_id).is_none());
}
