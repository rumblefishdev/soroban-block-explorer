use super::*;

fn db_err() -> EnrichError {
    EnrichError::Database(clickhouse::error::Error::Custom("boom".to_owned()))
}

#[test]
fn effective_chunk_no_limit_returns_chunk_size() {
    assert_eq!(effective_chunk(200, None, 0), 200);
    assert_eq!(effective_chunk(200, None, 1_000), 200);
}

#[test]
fn effective_chunk_caps_at_remaining_when_below_chunk_size() {
    assert_eq!(effective_chunk(200, Some(50), 0), 50);
    assert_eq!(effective_chunk(200, Some(50), 30), 20);
}

#[test]
fn effective_chunk_full_chunk_when_remaining_exceeds_chunk_size() {
    assert_eq!(effective_chunk(200, Some(1_000), 0), 200);
}

#[test]
fn limit_reached_no_limit_is_false() {
    assert!(!limit_reached(None, 0));
    assert!(!limit_reached(None, 1_000_000));
}

#[test]
fn limit_reached_below_cap_is_false() {
    assert!(!limit_reached(Some(50), 49));
}

#[test]
fn limit_reached_at_or_above_cap_is_true() {
    assert!(limit_reached(Some(50), 50));
    assert!(limit_reached(Some(50), 51));
}

#[test]
fn report_new_initialises_counters_to_zero() {
    let r = BackfillReport::new("sep1-assets");
    assert_eq!(r.kind, "sep1-assets");
    assert_eq!(r.processed, 0);
    assert_eq!(r.enriched, 0);
    assert_eq!(r.sentinel, 0);
    assert_eq!(r.unreachable, 0);
    assert_eq!(r.db_failed, 0);
}

#[test]
fn tally_real_and_sentinel_increment_separately() {
    let mut r = BackfillReport::new("sep1-assets");
    tally(&mut r, "k", Ok(EnrichOutcome::Real));
    tally(&mut r, "k2", Ok(EnrichOutcome::Sentinel));
    assert_eq!(r.processed, 2);
    assert_eq!(r.enriched, 1);
    assert_eq!(r.sentinel, 1);
    assert_eq!(r.unreachable, 0);
    assert_eq!(r.db_failed, 0);
}

#[test]
fn tally_transient_increments_unreachable() {
    let mut r = BackfillReport::new("sep1-assets");
    tally(
        &mut r,
        "1-USDC-42-7",
        Err(EnrichError::Transient("dns fail".into())),
    );
    assert_eq!(r.processed, 1);
    assert_eq!(r.unreachable, 1);
}

#[test]
fn tally_database_error_increments_db_failed() {
    let mut r = BackfillReport::new("sep1-assets");
    tally(&mut r, "k", Err(db_err()));
    assert_eq!(r.processed, 1);
    assert_eq!(r.db_failed, 1);
}

#[test]
fn tally_mixed_outcomes_accumulate_correctly() {
    let mut r = BackfillReport::new("nft-metadata");
    tally(&mut r, "a", Ok(EnrichOutcome::Real));
    tally(&mut r, "b", Ok(EnrichOutcome::Sentinel));
    tally(
        &mut r,
        "c",
        Err(EnrichError::Transient("upstream 502".into())),
    );
    tally(&mut r, "d", Err(db_err()));
    tally(&mut r, "e", Ok(EnrichOutcome::Real));
    assert_eq!(r.processed, 5);
    assert_eq!(r.enriched, 2);
    assert_eq!(r.sentinel, 1);
    assert_eq!(r.unreachable, 1);
    assert_eq!(r.db_failed, 1);
}

/// CH-backed candidate-query smoke (task 0231 step 5): `select_sep1_chunk`
/// returns only classic assets (type 1; a SAC is a facet, ADR 0051) with no
/// `asset_enrichment` row, skips the enriched one, excludes native;
/// `--force-retry` drops the NOT-IN. `#[ignore]` (needs live local CH).
#[tokio::test]
#[ignore = "needs live local ClickHouse"]
async fn select_sep1_chunk_skips_enriched_and_native() {
    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    // 1 classic un-enriched, 1 classic-with-SAC-facet enriched (type 1 now,
    // ADR 0051), 1 native (type-excluded).
    client
        .query(
            "INSERT INTO assets \
             (asset_type, asset_code, issuer_id, contract_id) \
             VALUES (1,'AAA',7001,0), \
                    (1,'BBB',7002,0), \
                    (0,'',0,0)",
        )
        .execute()
        .await
        .expect("seed assets");
    client
        .query(
            "INSERT INTO asset_enrichment \
             (asset_type, asset_code, issuer_id, contract_id, icon_url, name, version) \
             VALUES (1,'BBB',7002,0,'i','n',now64(3))",
        )
        .execute()
        .await
        .expect("seed enrichment");

    // Standard → only AAA (BBB enriched-skip, native type-skip).
    let got = select_sep1_chunk(&client, None, 100, DrainMode::Untried)
        .await
        .expect("candidate query");
    assert_eq!(
        got,
        vec![AssetKey {
            asset_type: 1,
            asset_code: "AAA".into(),
            issuer_id: 7001,
            contract_id: 0,
        }],
    );

    // Force → AAA + BBB (NOT-IN dropped), still no native.
    let forced = select_sep1_chunk(&client, None, 100, DrainMode::All)
        .await
        .expect("force-retry query");
    assert_eq!(forced.len(), 2);
    assert!(forced.iter().all(|k| k.asset_type == 1));

    client
        .query(
            "ALTER TABLE assets DELETE WHERE asset_code IN ('AAA','BBB') \
             OR (asset_type = 0 AND asset_code = '')",
        )
        .execute()
        .await
        .expect("cleanup assets");
    client
        .query("ALTER TABLE asset_enrichment DELETE WHERE asset_code = 'BBB'")
        .execute()
        .await
        .expect("cleanup enrichment");
}

/// NFT counterpart — `select_nft_chunk` skips the enriched key, returns the
/// rest; `--force-retry` returns all. `#[ignore]`.
#[tokio::test]
#[ignore = "needs live local ClickHouse"]
async fn select_nft_chunk_skips_enriched() {
    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    client
        .query("INSERT INTO nfts (contract_id, token_id) VALUES (6001,'1'),(6002,'2')")
        .execute()
        .await
        .expect("seed nfts");
    client
        .query(
            "INSERT INTO nft_enrichment \
             (contract_id, token_id, name, media_url, collection_name, version) \
             VALUES (6001,'1','n','m','c',now64(3))",
        )
        .execute()
        .await
        .expect("seed nft enrichment");

    let got = select_nft_chunk(&client, None, 100, DrainMode::Untried)
        .await
        .expect("candidate query");
    assert_eq!(
        got,
        vec![NftKey {
            contract_id: 6002,
            token_id: "2".into(),
        }],
    );

    let forced = select_nft_chunk(&client, None, 100, DrainMode::All)
        .await
        .expect("force-retry");
    assert_eq!(forced.len(), 2);

    client
        .query("ALTER TABLE nfts DELETE WHERE contract_id IN (6001,6002)")
        .execute()
        .await
        .expect("cleanup nfts");
    client
        .query("ALTER TABLE nft_enrichment DELETE WHERE contract_id = 6001")
        .execute()
        .await
        .expect("cleanup enrichment");
}

/// `--retry-sentinels` mode: candidate = existing all-`''` rows only.
/// A real row and a PARTIAL (real icon + `''` name) must be excluded —
/// proving partials are never re-fetched/clobbered. `#[ignore]`.
#[tokio::test]
#[ignore = "needs live local ClickHouse"]
async fn select_sep1_chunk_sentinels_excludes_real_and_partial() {
    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
    client
        .query(
            "INSERT INTO asset_enrichment \
             (asset_type, asset_code, issuer_id, contract_id, icon_url, name, version) VALUES \
             (1,'REAL',8001,0,'https://i','n',now64(3)), \
             (1,'PART',8002,0,'https://i','',now64(3)), \
             (1,'SENT',8003,0,'','',now64(3))",
        )
        .execute()
        .await
        .expect("seed enrichment");

    let got = select_sep1_chunk(&client, None, 100, DrainMode::Sentinels)
        .await
        .expect("sentinels query");
    assert_eq!(
        got,
        vec![AssetKey {
            asset_type: 1,
            asset_code: "SENT".into(),
            issuer_id: 8003,
            contract_id: 0,
        }],
        "only the all-'' row — real + partial excluded"
    );

    client
        .query("ALTER TABLE asset_enrichment DELETE WHERE asset_code IN ('REAL','PART','SENT')")
        .execute()
        .await
        .expect("cleanup");
}
