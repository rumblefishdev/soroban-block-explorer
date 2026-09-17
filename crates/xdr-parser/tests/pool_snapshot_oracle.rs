//! Differential check of the pool extractor against an independent decode
//! (task 0210). Not a CI test: the inputs are mainnet ledgers from the public
//! archive, too large to commit.
//!
//! `POOL_ORACLE_DIR` holds `xdr/*--<ledger>.xdr.zst` (from
//! `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/`) and
//! `expected/<ledger>.tsv`: `pool_id hex, reserve_a, reserve_b, total_shares`
//! for every classic pool the ledger touches, at ledger close. The expectation
//! comes from `stellar xdr decode` JSON read by a separate script — the last
//! change of each pool in apply order, 0/0/0 after a `removed` — so it shares
//! no code with this crate.
//!
//! Run: `POOL_ORACLE_DIR=<dir> cargo test -p xdr-parser --test pool_snapshot_oracle -- --nocapture`

use std::collections::BTreeMap;
use std::path::Path;

use stellar_xdr::{LedgerCloseMeta, TransactionMeta};
use xdr_parser::{
    decompress_zstd, dedup_final_pool_snapshots, deserialize_batch, extract_ledger_entry_changes,
    extract_liquidity_pools,
};

type Pools = BTreeMap<String, (i64, i64, i64)>;

#[test]
fn every_pool_ends_each_ledger_where_the_independent_decode_says() {
    let Ok(dir) = std::env::var("POOL_ORACLE_DIR") else {
        eprintln!("POOL_ORACLE_DIR not set — skipping the pool snapshot oracle");
        return;
    };
    let dir = Path::new(&dir);
    let mut ledgers = 0;
    let mut pools = 0;
    let mut mismatches = Vec::new();
    for entry in std::fs::read_dir(dir.join("xdr")).expect("xdr dir") {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(seq) = name
            .strip_suffix(".xdr.zst")
            .and_then(|n| n.split("--").nth(1))
        else {
            continue;
        };
        let expected = read_expected(&dir.join("expected").join(format!("{seq}.tsv")));
        let ours = extract(&std::fs::read(&path).expect("read ledger"));
        ledgers += 1;
        pools += expected.len();
        if ours != expected {
            let ids: std::collections::BTreeSet<&String> =
                expected.keys().chain(ours.keys()).collect();
            for id in ids {
                if expected.get(id) != ours.get(id) {
                    mismatches.push(format!(
                        "{seq} {id}: expected {:?}, ours {:?}",
                        expected.get(id),
                        ours.get(id)
                    ));
                }
            }
        }
    }
    eprintln!("{ledgers} ledgers, {pools} pool end states compared");
    assert!(ledgers > 0, "no ledgers under {}", dir.display());
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn read_expected(path: &Path) -> Pools {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let n = |i: usize| c[i].parse::<i64>().expect("number");
            (c[0].to_string(), (n(1), n(2), n(3)))
        })
        .collect()
}

/// Production wiring (`indexer::handler::process`): extract per transaction,
/// aggregate, dedup at ledger scope.
fn extract(zst: &[u8]) -> Pools {
    let batch = deserialize_batch(&decompress_zstd(zst).expect("zstd")).expect("batch");
    let mut snapshots = Vec::new();
    for meta in &batch.ledger_close_metas {
        let seq = ledger_seq(meta);
        for tx in tx_metas(meta) {
            let changes = extract_ledger_entry_changes(&tx, "tx", seq, 0);
            snapshots.extend(extract_liquidity_pools(&changes).1);
        }
    }
    dedup_final_pool_snapshots(snapshots)
        .into_iter()
        .map(|s| {
            let r = |k: &str| s.reserves[k].as_i64().expect("reserve");
            let shares = s.total_shares.parse::<i64>().expect("shares");
            (s.pool_id, (r("a"), r("b"), shares))
        })
        .collect()
}

fn ledger_seq(meta: &LedgerCloseMeta) -> u32 {
    match meta {
        LedgerCloseMeta::V0(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(v) => v.ledger_header.header.ledger_seq,
    }
}

fn tx_metas(meta: &LedgerCloseMeta) -> Vec<TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| p.tx_apply_processing.clone())
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| p.tx_apply_processing.clone())
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| p.tx_apply_processing.clone())
            .collect(),
    }
}
