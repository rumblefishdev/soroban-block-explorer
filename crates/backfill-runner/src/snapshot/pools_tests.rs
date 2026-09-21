use super::*;
use crate::snapshot::archive::SnapshotRecord;
use base64::Engine;
use stellar_xdr::{LedgerEntryExt, LedgerKey, LedgerKeyLiquidityPool, Limits, PoolId, ReadXdr};

/// A mainnet pool entry from `getLedgerEntries` (2026-09-16), with the rows
/// production's live writer stored for the same pool at the same ledger.
struct MainnetPool {
    data: &'static str,
    last_modified: u32,
    reserves_and_shares: (i128, i128, i128),
    legs: [i64; 2],
    asset_b_code: &'static str,
}

const MAINNET_POOLS: [MainnetPool; 2] = [
    MainnetPool {
        // native / 1x
        data: "AAAABQZoWEI2qhtXR0FUeVkGO71wSeVLyMpEMOYL6YeV6ED0AAAAAAAAAAAAAAABMXgAAAAAAABNEIqB8v6z8TDxHjf+JD6GMAmCpBH/M2/w2d1C1NZ6tAAAAB4AAAAAAEY7iQAAAAAFrQ+BAAAAAACbaLwAAAAAAAAAAQ==",
        last_modified: 64_454_667,
        reserves_and_shares: (4_602_761, 95_227_777, 10_184_892),
        legs: [-6_959_166_271_784_855_184, 459_826_772_677_602_557],
        asset_b_code: "1x",
    },
    MainnetPool {
        // XXA / yXLM
        data: "AAAABU3BU3qc8c0UgbY3gp6w45MVox7I19CCcyOcIWKT9ZvpAAAAAAAAAAFYWEEAAAAAALh5cFAWaLQ6ZlreaIAMkEayxj7HzipA3vLbAaJXUi9wAAAAAXlYTE0AAAAAIjbXcP4NPgFSGXXVz3rEhCtwldaxqddo0+mmMumZBr4AAAAeAAABBxdtmPsAAAACzHn+wgAAAA7dkMMZAAAAAAAAABQ=",
        last_modified: 64_454_668,
        reserves_and_shares: (1_129_969_457_403, 12_020_481_730, 63_846_794_009),
        legs: [6_882_095_055_627_461_881, 258_332_573_254_456_524],
        asset_b_code: "yXLM",
    },
];

fn entry(data_b64: &str, last_modified: u32) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: last_modified,
        data: LedgerEntryData::from_xdr(
            base64::engine::general_purpose::STANDARD
                .decode(data_b64)
                .unwrap(),
            Limits::none(),
        )
        .unwrap(),
        ext: LedgerEntryExt::V0,
    }
}

#[test]
fn a_checkpoint_pool_builds_the_rows_production_wrote_for_it() {
    for pool in &MAINNET_POOLS {
        let e = entry(pool.data, pool.last_modified);
        let (pools, snapshots) = rows_for_entry(&e).unwrap();

        assert_eq!(snapshots.len(), 1);
        let s = &snapshots[0];
        assert_eq!(s.ledger_sequence, i64::from(pool.last_modified));
        assert_eq!(
            (s.reserve_a, s.reserve_b, s.total_shares),
            pool.reserves_and_shares
        );
        assert_eq!(s.gross_volume_a, None);

        assert_eq!(pools.len(), 1);
        let p = &pools[0];
        assert_eq!(p.pool_id, s.pool_id);
        assert_eq!(p.legs, pool.legs.to_vec());
        assert_eq!(p.asset_b_code, pool.asset_b_code);
        assert_eq!((p.pool_kind, p.fee_bps), (0, 30));
        assert_eq!(p.last_updated_ledger, i64::from(pool.last_modified));
    }
}

#[test]
fn stub_asset_ids_meet_the_pool_legs() {
    for pool in &MAINNET_POOLS {
        let e = entry(pool.data, pool.last_modified);
        let LedgerEntryData::LiquidityPool(lp) = &e.data else {
            panic!("not a pool entry");
        };
        let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &lp.body;
        for (asset, leg) in [
            (&cp.params.asset_a, pool.legs[0]),
            (&cp.params.asset_b, pool.legs[1]),
        ] {
            let (id, identity) = classic_asset(asset);
            assert_eq!(id, leg);
            // The seed's stub for this leg, built like live ingest builds it,
            // must define the same id (native needs no stub).
            if let Some((code, issuer)) = identity {
                let stub = db_clickhouse::persist::rows::AssetRow::staged(
                    1,
                    code,
                    ids::account_id(&issuer),
                    0,
                );
                assert_eq!(stub.id, leg);
            }
        }
    }
}

#[test]
fn only_a_missing_or_older_snapshot_needs_an_insert() {
    assert_eq!(need(None, 64_000_000), Need::Missing);
    assert_eq!(need(Some(63_999_999), 64_000_000), Need::Stale);
    assert_eq!(need(Some(64_000_000), 64_000_000), Need::Current);
    // A `state` read stamps our snapshot with the reading ledger, later than
    // the entry's own last modification.
    assert_eq!(need(Some(64_000_050), 64_000_000), Need::Current);
}

#[test]
fn only_a_pool_that_was_ours_before_the_checkpoint_is_reported_gone() {
    assert!(gone_with_reserves(true, false, 63_999_999, 64_000_000));
    // Created after the checkpoint, so not in it yet.
    assert!(!gone_with_reserves(true, false, 64_000_000, 64_000_000));
    assert!(!gone_with_reserves(true, true, 63_999_999, 64_000_000));
    assert!(!gone_with_reserves(false, false, 63_999_999, 64_000_000));
}

#[test]
fn a_removed_pool_seen_first_is_not_revived_by_an_older_entry() {
    let live = entry(MAINNET_POOLS[0].data, MAINNET_POOLS[0].last_modified);
    let LedgerEntryData::LiquidityPool(lp) = &live.data else {
        panic!("not a pool entry");
    };
    let dead = SnapshotRecord::Dead(Box::new(LedgerKey::LiquidityPool(LedgerKeyLiquidityPool {
        liquidity_pool_id: PoolId(lp.liquidity_pool_id.0.clone()),
    })));

    let mut state = NetworkState::default();
    state.absorb_record(&dead);
    state.absorb_record(&SnapshotRecord::Live(Box::new(live)));

    assert_eq!(state.live_pools(), 0);
    assert_eq!(state.pools.len(), 1);
    assert_eq!(state.superseded, 1);
}
