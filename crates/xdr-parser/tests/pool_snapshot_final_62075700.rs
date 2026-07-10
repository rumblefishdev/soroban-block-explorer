//! Integration regression for lore-0356: parsing a real mainnet ledger must
//! yield exactly ONE liquidity-pool snapshot per `(pool_id, ledger_sequence)` =
//! the ledger's FINAL (end-of-ledger) reserves — never a before/intermediate
//! image left to a non-deterministic DB dedup.
//!
//! Ledger 62075700 touches 41 pools; every one has before != final (all pure
//! swaps, LP shares unchanged). Expected finals were derived by re-parsing the
//! raw `LedgerCloseMeta` and validated 41/41 against Horizon's independently
//! parsed `liquidity_pool_trade` effects.
//!
//! Mirrors production wiring (`indexer::handler::process`): `extract_liquidity_pools`
//! runs per transaction, snapshots are aggregated, then `dedup_final_pool_snapshots`
//! collapses to the end-of-ledger image at ledger scope.
//!
//! Loads the ledger from `.temp/` if present (skipped in CI, where the archive
//! isn't shipped) or from `XDR_FIXTURE`. Fetch it with:
//!   aws s3 cp --no-sign-request \
//!     s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/FC4DB5FF--62016000-62079999/FC4CCCCB--62075700.xdr.zst \
//!     .temp/FC4DB5FF--62016000-62079999/FC4CCCCB--62075700.xdr.zst
//! The pure-logic guards live in `crates/xdr-parser/src/state.rs` unit tests and
//! run without a fixture.

use std::collections::BTreeMap;
use std::path::PathBuf;

use stellar_xdr::{LedgerCloseMeta, TransactionMeta};
use xdr_parser::{
    decompress_zstd, dedup_final_pool_snapshots, deserialize_batch, extract_ledger_entry_changes,
    extract_liquidity_pools,
};

const LEDGER: u32 = 62_075_700;

#[test]
fn ledger_62075700_yields_one_final_snapshot_per_pool() {
    let Some(path) = locate_fixture() else {
        eprintln!(
            "no XDR fixture for ledger 62075700 (set XDR_FIXTURE or place the \
             .xdr.zst under .temp/) — skipping"
        );
        return;
    };

    let bytes = std::fs::read(&path).expect("read fixture");
    let batch = deserialize_batch(&decompress_zstd(&bytes).expect("zstd")).expect("xdr batch");

    let Some(meta) = batch
        .ledger_close_metas
        .iter()
        .find(|m| ledger_seq(m) == LEDGER)
    else {
        eprintln!("fixture does not contain ledger {LEDGER} — skipping");
        return;
    };

    // Production wiring: per-tx extract → aggregate → ledger-scope dedup.
    let mut aggregated = Vec::new();
    for (hash, tx_meta) in tx_metas(meta) {
        let changes = extract_ledger_entry_changes(&tx_meta, &hash, LEDGER, 0);
        let (_pools, snapshots) = extract_liquidity_pools(&changes);
        aggregated.extend(snapshots);
    }
    let snapshots = dedup_final_pool_snapshots(aggregated);

    // 1) exactly one snapshot per pool — no leftover before/intermediate images.
    let mut by_pool: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &snapshots {
        assert_eq!(s.ledger_sequence, LEDGER);
        *by_pool.entry(s.pool_id.as_str()).or_default() += 1;
    }
    let dupes: Vec<_> = by_pool.iter().filter(|&(_, &n)| n > 1).collect();
    assert!(
        dupes.is_empty(),
        "duplicate (pool, ledger) snapshots: {dupes:?}"
    );
    assert_eq!(snapshots.len(), 41, "ledger 62075700 touches 41 pools");

    // 2) spot-check final reserve_a (stroops) for multi-op pools against the
    //    re-parsed + Horizon-validated golden values.
    let reserve_a = |pool: &str| -> i64 {
        snapshots
            .iter()
            .find(|s| s.pool_id == pool)
            .unwrap_or_else(|| panic!("pool {pool} missing"))
            .reserves
            .get("a")
            .and_then(|v| v.as_i64())
            .expect("reserve a")
    };
    // 0/YxT — 5 ops; before 1466407525765, final:
    assert_eq!(
        reserve_a("75110bc547638e69d36a7fecb950e1320f3229b5a4f4b1cc7bd9cebcb9322331"),
        1_466_363_420_822
    );
    // 0/aTTaiN — 5 ops:
    assert_eq!(
        reserve_a("40e4f6cee0b4dfe3ea962a103bdc51a8f7bad0ea8f8dc0218a1f60249ddc9eb7"),
        671_330_722_202
    );
    // AFR/yXLM — 3 ops:
    assert_eq!(
        reserve_a("0e6c86b8bdd7f3510fff44a9b0b8ade4bf854e6757ff19b87ae862b0b1ff67a4"),
        191_943_931_239_552
    );
}

fn ledger_seq(meta: &LedgerCloseMeta) -> u32 {
    match meta {
        LedgerCloseMeta::V0(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(v) => v.ledger_header.header.ledger_seq,
    }
}

/// Per-tx `(hash, meta)` in apply order — mirrors examples/decode_pool_ledger.rs.
/// Per-arm map: the `tx_processing` element type differs across V0/V1/V2.
fn tx_metas(meta: &LedgerCloseMeta) -> Vec<(String, TransactionMeta)> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| {
                (
                    hex::encode(p.result.transaction_hash.0),
                    p.tx_apply_processing.clone(),
                )
            })
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| {
                (
                    hex::encode(p.result.transaction_hash.0),
                    p.tx_apply_processing.clone(),
                )
            })
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| {
                (
                    hex::encode(p.result.transaction_hash.0),
                    p.tx_apply_processing.clone(),
                )
            })
            .collect(),
    }
}

fn locate_fixture() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("XDR_FIXTURE") {
        let pb = PathBuf::from(&p);
        assert!(pb.is_file(), "XDR_FIXTURE set but not a file: {p}");
        return Some(pb);
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.temp/FC4DB5FF--62016000-62079999/FC4CCCCB--62075700.xdr.zst");
    p.is_file().then_some(p)
}
