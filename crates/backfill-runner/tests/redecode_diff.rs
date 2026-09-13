//! Task 0540 — rollout gate 7b: re-decode archive ledgers, diff against the table.
//!
//! Runs the backfill's own per-ledger path on raw archive files —
//! `indexer::handler::process::parse_ledger`, then
//! `db_clickhouse::persist::stage::prepare_with_sac_overrides` with the inputs
//! `sink.rs::write_ledger` passes under `--only` — and writes the three
//! value-flow tables as TSV, sorted by their sort keys, in the column order the
//! matching production `SELECT … FINAL … FORMAT TSV` returns. `diff` against
//! that export IS the comparison; this file decides nothing.
//!
//! What a clean diff proves: every row the table holds for these ledgers is
//! exactly what the decoder produces from the archive today, and no row is
//! missing or extra — surrogates, `*_kind`, muxed ids from the envelope and
//! memos included. It exercises the write path (staging, targeted writer,
//! RMT collapse), which gate 7a's counts cannot see.
//!
//! What it does not prove: that the decoder is right. Same code on both sides.
//! That is gate 7c (T11), against ledger state read from the network.
//!
//! Gated on `LEDGER_CACHE_DIR` (archive files as `<HEX>--<seq>.xdr.zst`) and
//! `REDECODE_OUT_DIR`; skips when either is unset or a file is missing, so
//! `cargo test` stays offline.
//!
//! ```bash
//! LEDGER_CACHE_DIR=.temp/ledger-cache REDECODE_OUT_DIR=.temp/redecode \
//!     cargo test -p backfill-runner --test redecode_diff -- --nocapture
//! ```

use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

/// Thirty ledgers spread evenly over the backfilled range (the value-flow
/// oracle's sample, protocols 20–27), its three named edge cases, and one
/// ledger per case the counts in gate 7a cannot distinguish.
const LEDGERS: &[u32] = &[
    50_457_424, 50_933_656, 51_409_888, 51_886_120, 52_362_352, 52_838_584, 53_314_816, 53_791_048,
    54_267_280, 54_743_512, 55_219_744, 55_695_976, 56_172_208, 56_648_440, 57_124_672, 57_600_904,
    58_077_136, 58_553_368, 59_029_600, 59_505_832, 59_982_064, 60_458_296, 60_934_528, 61_410_760,
    61_886_992, 62_363_224, 62_839_456, 63_315_688, 63_791_920, 64_268_152,
    64_249_110, // two byte-identical transfers inside one operation
    64_260_088, // six-hop classic-pool arbitrage
    60_000_138, // 86 payment operations in one transaction
    52_510_752, 52_510_759, 52_558_370, 52_570_526, 52_570_585, // unrecognised_topics rejects
    55_900_795, // a `TRANSFER` verb — case-variant spelling
    63_972_580, // muxed destination
    63_507_557, // muxed source
    63_051_829, // non-fungible movement, NULL amount
    62_750_148, // MEMO_TEXT that is not UTF-8, stored as `text_hex`
    61_368_406, // claimable balance as an endpoint
    64_317_019, // written by the backfill AND the live indexer
];

fn cache_path(dir: &Path, seq: u32) -> PathBuf {
    dir.join(format!("{:08X}--{}.xdr.zst", u32::MAX - seq, seq))
}

/// ClickHouse's TSV spelling of a NULL.
fn opt<T: Display>(v: Option<T>) -> String {
    v.map_or_else(|| "\\N".to_string(), |x| x.to_string())
}

/// `hex(memo)` as ClickHouse prints it: the memo can hold bytes TSV escaping
/// would rewrite, so both sides compare the hex.
fn hex_upper(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02X}")).collect()
}

#[test]
fn redecode_value_flow_tables() {
    let (Some(cache), Some(out)) = (
        std::env::var_os("LEDGER_CACHE_DIR").map(PathBuf::from),
        std::env::var_os("REDECODE_OUT_DIR").map(PathBuf::from),
    ) else {
        eprintln!("SKIP: set LEDGER_CACHE_DIR and REDECODE_OUT_DIR");
        return;
    };
    let missing: Vec<u32> = LEDGERS
        .iter()
        .copied()
        .filter(|s| !cache_path(&cache, *s).exists())
        .collect();
    if !missing.is_empty() {
        eprintln!("SKIP: {} archive files missing: {missing:?}", missing.len());
        return;
    }
    fs::create_dir_all(&out).expect("create REDECODE_OUT_DIR");

    let mut transfers = Vec::new();
    let mut memos = Vec::new();
    let mut event_ops = Vec::new();

    for &seq in LEDGERS {
        let bytes = fs::read(cache_path(&cache, seq)).expect("read archive file");
        let xdr = xdr_parser::decompress_zstd(&bytes).expect("zstd");
        let batch = xdr_parser::deserialize_batch(&xdr).expect("LedgerCloseMetaBatch");
        for meta in batch.ledger_close_metas.iter() {
            let parsed = indexer::handler::process::parse_ledger(meta);
            // Mirrors `sink.rs::write_ledger` under `--only`: the SAC map is
            // skipped there (no targetable table reads it), and the three
            // prior-verdict maps are empty on every backfill path.
            let sac_classic = HashMap::new();
            let staged = db_clickhouse::persist::stage::prepare_with_sac_overrides(
                &db_clickhouse::persist::stage::StageInputs {
                    ledger: &parsed.ledger,
                    transactions: &parsed.transactions,
                    operations: &parsed.operations,
                    events: &parsed.events,
                    invocations: &parsed.invocations,
                    contract_interfaces: &parsed.contract_interfaces,
                    contract_deployments: &parsed.contract_deployments,
                    account_states: &parsed.account_states,
                    liquidity_pools: &parsed.liquidity_pools,
                    pool_snapshots: &parsed.pool_snapshots,
                    assets: &parsed.assets,
                    nfts: &parsed.nfts,
                    nft_events: &parsed.nft_events,
                    lp_positions: &parsed.lp_positions,
                    contract_metadata_writes: &parsed.contract_metadata_writes,
                    soroban_token_balances: &parsed.soroban_token_balances,
                    pool_family_writes: &parsed.pool_family_writes,
                    sac_classic: &sac_classic,
                    sac_overrides: &parsed.sac_overrides,
                    prior_wasm_verdicts: &HashMap::new(),
                    prior_contract_verdicts: &HashMap::new(),
                    prior_contract_rows: &HashMap::new(),
                    asset_transfers: &parsed.asset_transfers,
                },
            )
            .expect("stage");

            for r in &staged.asset_transfer_rows {
                transfers.push((
                    (
                        r.ledger_sequence,
                        r.application_order,
                        r.op_index,
                        r.event_pos_in_op,
                    ),
                    [
                        r.ledger_sequence.to_string(),
                        r.application_order.to_string(),
                        r.op_index.to_string(),
                        r.event_pos_in_op.to_string(),
                        r.event_index.to_string(),
                        r.asset_id.to_string(),
                        opt(r.amount),
                        opt(r.from_id),
                        r.from_kind.clone(),
                        opt(r.from_muxed_id),
                        opt(r.to_id),
                        r.to_kind.clone(),
                        opt(r.to_muxed_id),
                        r.verb.clone(),
                    ]
                    .join("\t"),
                ));
            }
            for r in &staged.transaction_memo_rows {
                memos.push((
                    (r.ledger_sequence, r.application_order),
                    [
                        r.ledger_sequence.to_string(),
                        r.application_order.to_string(),
                        r.memo_type.clone(),
                        hex_upper(&r.memo),
                    ]
                    .join("\t"),
                ));
            }
            for r in &staged.event_op_rows {
                event_ops.push((
                    (r.ledger_sequence, r.application_order, r.event_index),
                    [
                        r.ledger_sequence.to_string(),
                        r.application_order.to_string(),
                        r.event_index.to_string(),
                        r.op_index.to_string(),
                        r.event_pos_in_op.to_string(),
                    ]
                    .join("\t"),
                ));
            }
        }
    }

    transfers.sort();
    memos.sort();
    event_ops.sort();
    let write = |name: &str, lines: Vec<String>| {
        let mut body = lines.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        fs::write(out.join(name), body).expect("write tsv");
    };
    println!(
        "{} ledgers: {} transfers, {} memos, {} event ops",
        LEDGERS.len(),
        transfers.len(),
        memos.len(),
        event_ops.len()
    );
    write(
        "asset_transfers.tsv",
        transfers.into_iter().map(|(_, l)| l).collect(),
    );
    write(
        "transaction_memos.tsv",
        memos.into_iter().map(|(_, l)| l).collect(),
    );
    write(
        "soroban_event_ops.tsv",
        event_ops.into_iter().map(|(_, l)| l).collect(),
    );
}
