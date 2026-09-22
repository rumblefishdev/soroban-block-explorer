//! Task 0573: event extraction on real ledgers, frozen before its rewrite.
//!
//! Each fixture is one `LedgerCloseMetaBatch` object from the public archive
//! (`s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/…`). The expected
//! output was written by the extraction as it stood before task 0573: per
//! event, its transaction, where it came from, its stellar-rpc id, its type
//! and a hash of its contract, topics and data. A rewrite must reproduce it
//! line for line.
//!
//! - 50,500,000 — protocol 20, refunds `AfterTx`;
//! - 58,762,517 — the last ledger whose refunds are `AfterTx`;
//! - 58,762,518 — the first whose refunds are `AfterAllTxs`;
//! - 64,550,000 — protocol 28.
//!
//! All four carry V4 metas, protocol 20 included: the archive re-exports old
//! ledgers in the current format.
//!
//! Rewrite the expected files only when a change is meant to alter them:
//!
//! ```bash
//! EVENT_GOLDEN_WRITE=1 cargo test -p xdr-parser --test event_extraction_golden
//! ```

use std::fmt::Write as _;

use domain::ContractEventType;
use sha2::{Digest, Sha256};
use stellar_xdr::{LedgerCloseMeta, TransactionMeta};
use xdr_parser::{EventOrigin, LedgerEvents};

const LEDGERS: [(u32, &[u8]); 4] = [
    (
        50_500_000,
        include_bytes!("fixtures/ledgers/ledger_50500000.xdr.zst"),
    ),
    (
        58_762_517,
        include_bytes!("fixtures/ledgers/ledger_58762517.xdr.zst"),
    ),
    (
        58_762_518,
        include_bytes!("fixtures/ledgers/ledger_58762518.xdr.zst"),
    ),
    (
        64_550_000,
        include_bytes!("fixtures/ledgers/ledger_64550000.xdr.zst"),
    ),
];

#[test]
fn event_extraction_matches_the_frozen_output() {
    let write = std::env::var_os("EVENT_GOLDEN_WRITE").is_some();
    for (ledger, raw) in LEDGERS {
        let path = format!(
            "{}/tests/fixtures/ledgers/event_extraction/{ledger}.tsv",
            env!("CARGO_MANIFEST_DIR")
        );
        let actual = dump(raw);
        if write {
            std::fs::write(&path, &actual).expect("write the expected output");
            continue;
        }
        let expected = std::fs::read_to_string(&path).expect("expected output");
        if let Some((n, (e, a))) = expected
            .lines()
            .zip(actual.lines())
            .enumerate()
            .find(|(_, (e, a))| e != a)
        {
            panic!(
                "ledger {ledger}, line {}:\nexpected {e}\n  actual {a}",
                n + 1
            );
        }
        assert_eq!(
            expected.lines().count(),
            actual.lines().count(),
            "ledger {ledger}: line count"
        );
    }
}

fn dump(raw: &[u8]) -> String {
    let xdr = xdr_parser::decompress_zstd(raw).expect("zstd");
    let batch = xdr_parser::deserialize_batch(&xdr).expect("LedgerCloseMetaBatch");
    let meta = &batch.ledger_close_metas[0];
    let header = match meta {
        LedgerCloseMeta::V0(v) => &v.ledger_header.header,
        LedgerCloseMeta::V1(v) => &v.ledger_header.header,
        LedgerCloseMeta::V2(v) => &v.ledger_header.header,
    };
    let metas = tx_metas(meta);
    let mut out = format!(
        "# ledger {} protocol {} transactions {}\n",
        header.ledger_seq,
        header.ledger_version,
        metas.len()
    );
    let events = LedgerEvents::new(header.ledger_seq, 0, &metas);
    for tx in 0..metas.len() {
        let tx_events = events.extract(tx, "");
        for e in &tx_events.events {
            let origin = match e.origin {
                EventOrigin::Transaction(stage) => format!("transaction {stage:?}"),
                EventOrigin::Operation(op) => format!("operation {op}"),
            };
            let id = e.event_id.to_rpc_string();
            line(
                &mut out,
                tx,
                &origin,
                &id,
                e.event_type,
                &e.contract_id,
                &e.topics,
                &e.data,
            );
        }
        for d in &tx_events.diagnostic {
            line(
                &mut out,
                tx,
                "diagnostic",
                "-",
                d.event_type,
                &d.contract_id,
                &d.topics,
                &d.data,
            );
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn line(
    out: &mut String,
    tx: usize,
    origin: &str,
    id: &str,
    event_type: ContractEventType,
    contract_id: &Option<String>,
    topics: &serde_json::Value,
    data: &serde_json::Value,
) {
    let body = Sha256::digest(format!(
        "{}\n{topics}\n{data}",
        contract_id.as_deref().unwrap_or("-")
    ));
    writeln!(
        out,
        "{tx}\t{origin}\t{id}\t{event_type:?}\t{}",
        hex::encode(&body[..8])
    )
    .expect("write to a String");
}

/// Every transaction's meta, in apply order.
fn tx_metas(meta: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
    }
}
