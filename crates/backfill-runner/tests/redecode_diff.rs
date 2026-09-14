//! Task 0540 — rollout gate 7b: re-decode archive ledgers, diff against the table.
//!
//! Runs the backfill's own per-ledger path on raw archive files —
//! `indexer::handler::process::parse_ledger`, then
//! `db_clickhouse::persist::stage::prepare_with_sac_overrides` with the inputs
//! `sink.rs::write_ledger` passes under `--only` — and writes the targetable
//! tables as TSV, sorted by their sort keys, in the column order the matching
//! production `SELECT … FINAL … FORMAT TSV` returns. `diff` against that export
//! IS the comparison; this file decides nothing.
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
//! `cargo test` stays offline. Paths must be absolute: cargo runs integration
//! tests from the package directory. `STELLAR_NETWORK_PASSPHRASE` must be set,
//! as it is for the backfill.
//!
//! ```bash
//! LEDGER_CACHE_DIR=$PWD/.temp/ledger-cache REDECODE_OUT_DIR=$PWD/.temp/redecode \
//!     STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015" \
//!     cargo test -p backfill-runner --test redecode_diff -- --nocapture
//! ```
//!
//! Archive files come from the public bucket (no credentials):
//!
//! ```bash
//! aws s3 cp --no-sign-request \
//!     s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/<HEX>--<start>-<end>/<HEX>--<seq>.xdr.zst <dir>/
//! ```
//!
//! where `<start>` is `seq - seq % 64000`, `<end>` is `start + 63999`, and each
//! `<HEX>` is `u32::MAX` minus the number after it, as eight uppercase digits.
//!
//! The production side, `L` being the comma-separated ledger list. Sort both
//! files the same way (`LC_ALL=C sort`) before `cmp` when an export is split by
//! partition:
//!
//! ```sql
//! SELECT ledger_sequence, application_order, op_index, event_pos_in_op, event_index, asset_id,
//!        amount, from_id, from_kind, from_muxed_id, to_id, to_kind, to_muxed_id, verb
//! FROM asset_transfers FINAL WHERE ledger_sequence IN (L)
//! ORDER BY ledger_sequence, application_order, op_index, event_pos_in_op FORMAT TSV;
//! SELECT ledger_sequence, application_order, memo_type, hex(memo)
//! FROM transaction_memos FINAL WHERE ledger_sequence IN (L)
//! ORDER BY ledger_sequence, application_order FORMAT TSV;
//! SELECT ledger_sequence, application_order, event_index, op_index, event_pos_in_op
//! FROM soroban_event_ops FINAL WHERE ledger_sequence IN (L)
//! ORDER BY ledger_sequence, application_order, event_index FORMAT TSV;
//! SELECT hex(pool_id), ledger_sequence, reserves, plane_id
//! FROM pool_state_changes FINAL WHERE ledger_sequence IN (L)
//! ORDER BY hex(pool_id), plane_id, ledger_sequence FORMAT TSV;
//! SELECT hex(pool_id), ledger_sequence, transaction_id, application_order, asset_id, amount
//! FROM lp_operation_amounts FINAL WHERE ledger_sequence IN (L)  -- per partition: FINAL over many is slow
//! ORDER BY hex(pool_id), ledger_sequence, transaction_id, application_order, asset_id FORMAT TSV;
//! -- the two state tables: the current version of chosen pools, compared with the
//! -- re-decoded row whose version ledger equals it (`*.all-versions.tsv`)
//! SELECT hex(pool_id), asset_a_type, asset_a_code, asset_a_issuer_id, asset_b_type, asset_b_code,
//!        asset_b_issuer_id, fee_bps, last_updated_ledger, pool_kind, legs, deployment_id, pool_type_raw
//! FROM liquidity_pools FINAL WHERE hex(pool_id) IN (…) ORDER BY hex(pool_id) FORMAT TSV;
//! SELECT hex(pool_id), plane_id, share_token_id, total_shares, derived_at_ledger
//! FROM pool_instance_state FINAL WHERE hex(pool_id) IN (…) ORDER BY hex(pool_id) FORMAT TSV;
//! ```
//!
//! First run (2026-09-13, task 0540 gate 7b and the 0518 pool tables): all seven
//! identical on the 92 ledgers below.

use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use db_clickhouse::persist::stage::{StageInputs, StagedLedger, prepare_with_sac_overrides};

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

/// Task 0518 — ledgers for the pool tables, on top of [`LEDGERS`]: the ledger
/// with the most `pool_state_changes` rows in each million of the range, and
/// the ledgers that wrote the current version of 12 `pool_instance_state` rows
/// and 20 `liquidity_pools` rows (10 soroban, 10 classic). A state table holds
/// one version per pool, so only its version ledger can be compared.
const POOL_LEDGERS: &[u32] = &[
    50_598_488, 50_693_320, 50_840_574, 51_206_668, 51_288_833, 51_288_881, 51_288_896, 52_730_973,
    52_881_004, 53_748_035, 54_086_582, 54_650_930, 55_987_569, 56_092_671, 56_194_788, 56_340_203,
    57_967_193, 58_030_496, 58_583_658, 59_001_034, 59_322_903, 59_810_071, 60_268_456, 60_281_019,
    60_834_082, 61_341_388, 61_460_340, 61_461_857, 61_462_173, 61_502_023, 62_146_218, 62_234_243,
    62_338_864, 62_338_871, 62_338_880, 62_588_355, 62_652_635, 63_043_939, 63_270_453, 64_055_926,
    64_102_937, 64_111_532, 64_202_829, 64_205_025, 64_291_226, 64_298_696, 64_316_480,
];

fn cache_path(dir: &Path, seq: u32) -> PathBuf {
    dir.join(format!("{:08X}--{}.xdr.zst", u32::MAX - seq, seq))
}

/// ClickHouse's TSV spelling of a NULL.
fn opt<T: Display>(v: Option<T>) -> String {
    v.map_or_else(|| "\\N".to_string(), |x| x.to_string())
}

/// `hex(…)` as ClickHouse prints it, for columns TSV escaping would rewrite.
fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// ClickHouse's TSV spelling of an `Array(IntN)`.
fn array<T: Display>(v: &[T]) -> String {
    let items: Vec<String> = v.iter().map(ToString::to_string).collect();
    format!("[{}]", items.join(","))
}

/// The two env paths, or a skip when a sample file is absent.
fn setup(ledgers: &[&[u32]]) -> Option<(PathBuf, PathBuf)> {
    let (Some(cache), Some(out)) = (
        std::env::var_os("LEDGER_CACHE_DIR").map(PathBuf::from),
        std::env::var_os("REDECODE_OUT_DIR").map(PathBuf::from),
    ) else {
        eprintln!("SKIP: set LEDGER_CACHE_DIR and REDECODE_OUT_DIR");
        return None;
    };
    let missing: Vec<u32> = ledgers
        .iter()
        .flat_map(|l| l.iter().copied())
        .filter(|s| !cache_path(&cache, *s).exists())
        .collect();
    if !missing.is_empty() {
        eprintln!("SKIP: {} archive files missing: {missing:?}", missing.len());
        return None;
    }
    fs::create_dir_all(&out).expect("create REDECODE_OUT_DIR");
    Some((cache, out))
}

/// One archive file through the backfill's per-ledger path.
fn stage_ledger(cache: &Path, seq: u32) -> Vec<StagedLedger> {
    let bytes = fs::read(cache_path(cache, seq)).expect("read archive file");
    let xdr = xdr_parser::decompress_zstd(&bytes).expect("zstd");
    let batch = xdr_parser::deserialize_batch(&xdr).expect("LedgerCloseMetaBatch");
    batch
        .ledger_close_metas
        .iter()
        .map(|meta| {
            let parsed = indexer::handler::process::parse_ledger(meta);
            // Mirrors `sink.rs::write_ledger` under `--only`: the SAC map is
            // skipped there (no targetable table reads it), and the three
            // prior-verdict maps are empty on every backfill path.
            let sac_classic = HashMap::new();
            prepare_with_sac_overrides(&StageInputs {
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
                executable_ref_targets: &parsed.executable_ref_targets,
                soroban_token_balances: &parsed.soroban_token_balances,
                pool_family_writes: &parsed.pool_family_writes,
                sac_classic: &sac_classic,
                sac_overrides: &parsed.sac_overrides,
                prior_wasm_verdicts: &HashMap::new(),
                prior_contract_verdicts: &HashMap::new(),
                prior_contract_rows: &HashMap::new(),
                asset_transfers: &parsed.asset_transfers,
            })
            .expect("stage")
        })
        .collect()
}

/// Sort by key, drop the key, write one line per row.
fn write_tsv<K: Ord>(out: &Path, name: &str, mut rows: Vec<(K, String)>) -> usize {
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let n = rows.len();
    let body: String = rows.into_iter().map(|(_, l)| l + "\n").collect();
    fs::write(out.join(name), body).expect("write tsv");
    n
}

#[test]
fn redecode_value_flow_tables() {
    let Some((cache, out)) = setup(&[LEDGERS]) else {
        return;
    };
    let mut transfers = Vec::new();
    let mut memos = Vec::new();
    let mut event_ops = Vec::new();

    for &seq in LEDGERS {
        for staged in stage_ledger(&cache, seq) {
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
                        hex_upper(r.memo.as_bytes()),
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

    println!(
        "{} ledgers: {} transfers, {} memos, {} event ops",
        LEDGERS.len(),
        write_tsv(&out, "asset_transfers.tsv", transfers),
        write_tsv(&out, "transaction_memos.tsv", memos),
        write_tsv(&out, "soroban_event_ops.tsv", event_ops),
    );
}

/// Task 0518 — the four pool tables the targeted write also carries. The two
/// fact tables compare row for row on every ledger; the two state tables are
/// written with every version this path stages, and the caller keeps, per
/// pool, the row whose version ledger equals the production row's.
#[test]
fn redecode_pool_tables() {
    let Some((cache, out)) = setup(&[LEDGERS, POOL_LEDGERS]) else {
        return;
    };
    let mut state_changes = Vec::new();
    let mut amounts = Vec::new();
    let mut pools = Vec::new();
    let mut instances = Vec::new();

    for &seq in LEDGERS.iter().chain(POOL_LEDGERS) {
        for staged in stage_ledger(&cache, seq) {
            for r in &staged.pool_state_change_rows {
                let id = hex_upper(&r.pool_id);
                state_changes.push((
                    (id.clone(), r.plane_id, r.ledger_sequence),
                    [
                        id,
                        r.ledger_sequence.to_string(),
                        array(&r.reserves),
                        r.plane_id.to_string(),
                    ]
                    .join("\t"),
                ));
            }
            for r in &staged.lp_amount_rows {
                let id = hex_upper(&r.pool_id);
                amounts.push((
                    (
                        id.clone(),
                        r.ledger_sequence,
                        r.transaction_id,
                        r.application_order,
                        r.asset_id,
                    ),
                    [
                        id,
                        r.ledger_sequence.to_string(),
                        r.transaction_id.to_string(),
                        r.application_order.to_string(),
                        r.asset_id.to_string(),
                        r.amount.to_string(),
                    ]
                    .join("\t"),
                ));
            }
            for r in &staged.pool_rows {
                let id = hex_upper(&r.pool_id);
                pools.push((
                    (id.clone(), r.last_updated_ledger),
                    [
                        id,
                        r.asset_a_type.to_string(),
                        r.asset_a_code.clone(),
                        r.asset_a_issuer_id.to_string(),
                        r.asset_b_type.to_string(),
                        r.asset_b_code.clone(),
                        r.asset_b_issuer_id.to_string(),
                        r.fee_bps.to_string(),
                        r.last_updated_ledger.to_string(),
                        r.pool_kind.to_string(),
                        array(&r.legs),
                        r.deployment_id.to_string(),
                        r.pool_type_raw.clone(),
                    ]
                    .join("\t"),
                ));
            }
            for r in &staged.pool_instance_state_rows {
                let id = hex_upper(&r.pool_id);
                instances.push((
                    (id.clone(), r.derived_at_ledger),
                    [
                        id,
                        r.plane_id.to_string(),
                        r.share_token_id.to_string(),
                        r.total_shares.to_string(),
                        r.derived_at_ledger.to_string(),
                    ]
                    .join("\t"),
                ));
            }
        }
    }

    println!(
        "{} ledgers: {} pool state changes, {} lp amounts, {} pool rows, {} instance rows",
        LEDGERS.len() + POOL_LEDGERS.len(),
        write_tsv(&out, "pool_state_changes.tsv", state_changes),
        write_tsv(&out, "lp_operation_amounts.tsv", amounts),
        write_tsv(&out, "liquidity_pools.all-versions.tsv", pools),
        write_tsv(&out, "pool_instance_state.all-versions.tsv", instances),
    );
}
