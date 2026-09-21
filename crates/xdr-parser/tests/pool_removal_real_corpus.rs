//! Classic liquidity pool changes on real mainnet transactions (task 0210).
//! Each fixture is one transaction's `TransactionMeta`, cut from the public
//! ledger archive (`s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/`)
//! and re-encoded with `stellar xdr encode --type TransactionMeta`.
//!
//! What the unit tests cannot show: how core really writes a pool's end.
//!
//! - `pool_revoked_last_holder` — tx `969b9f2d…`, ledger 63,599,411. An
//!   `allow_trust` revokes the last pool-share holder: the shares become
//!   claimable balances and the pool is erased inside one operation. The meta
//!   carries the start-of-operation `state` with full reserves and a `removed`,
//!   no zeroing `updated`. Stored as-is, this left 1,381 erased pools with their
//!   old reserves (checkpoint 64,469,759).
//! - `pool_withdraw_then_exit` — tx `0555f1e3…`, ledger 56,939,293. A withdraw
//!   (`state` → `updated` 0/0/0), then `change_trust` (`state` 0 → `removed`).
//! - `pool_removed_before_our_history` — tx `5a83eb78…`, ledger 54,369,113. The
//!   pool was last modified at 40,215,840, before our ingest floor; its removal
//!   is its only appearance, so the pool row must come from the `state` params.
//! - `pool_trustline_count_only` — tx `fd2de956…`, ledger 64,437,548. A
//!   `change_trust` changes only `pool_shares_trust_line_count`; reserves and
//!   shares stay, and still come from the `updated`.

use base64::Engine;
use stellar_xdr::{Limits, ReadXdr, TransactionMeta};
use xdr_parser::claimable_balance::extract_claimable_balances;
use xdr_parser::types::{ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot};
use xdr_parser::{
    dedup_final_pool_snapshots, extract_ledger_entry_changes, extract_liquidity_pools,
    extract_lp_positions,
};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/corpus/");

fn changes(fixture: &str, ledger: u32) -> Vec<xdr_parser::types::ExtractedLedgerEntryChange> {
    let b64 = std::fs::read_to_string(format!("{DIR}{fixture}.b64")).expect("fixture");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .expect("base64");
    let meta = TransactionMeta::from_xdr(bytes, Limits::none()).expect("decode meta");
    extract_ledger_entry_changes(&meta, "tx", ledger, 0)
}

/// The pool's rows the way `indexer::handler::process` stores them: its last
/// pool row, and its one end-of-ledger snapshot.
fn pool(
    fixture: &str,
    ledger: u32,
    pool_id: &str,
) -> (ExtractedLiquidityPool, ExtractedLiquidityPoolSnapshot) {
    let (pools, snapshots) = extract_liquidity_pools(&changes(fixture, ledger));
    let row = pools
        .into_iter()
        .rfind(|p| p.pool_id == pool_id)
        .expect("pool row");
    let mut snaps: Vec<_> = dedup_final_pool_snapshots(snapshots)
        .into_iter()
        .filter(|s| s.pool_id == pool_id)
        .collect();
    assert_eq!(snaps.len(), 1, "one snapshot per pool per ledger");
    (row, snaps.remove(0))
}

fn reserves(s: &ExtractedLiquidityPoolSnapshot) -> (i64, i64, &str) {
    (
        s.reserves["a"].as_i64().unwrap(),
        s.reserves["b"].as_i64().unwrap(),
        s.total_shares.as_str(),
    )
}

#[test]
fn a_revoked_last_holder_leaves_the_pool_at_zero() {
    let (row, snap) = pool(
        "pool_revoked_last_holder",
        63_599_411,
        "19a2d3d684d5dc534039cbc384e989ae57561b189d4e26f3e7d71cb1034da2ac",
    );
    assert_eq!(reserves(&snap), (0, 0, "0"));
    assert_eq!(row.asset_b["code"], "QPI");
    assert!(row.created_at_ledger.is_none());

    // The same operation pays the shares out: two claimable balances created,
    // and the pool-share position closed.
    let created = extract_claimable_balances(&changes("pool_revoked_last_holder", 63_599_411));
    let amounts: Vec<i64> = created
        .iter()
        .filter(|c| !c.closed)
        .map(|c| c.amount)
        .collect();
    assert!(amounts.contains(&5_153_048), "{amounts:?}");
    assert!(amounts.contains(&267_029_108_195_986), "{amounts:?}");
    let positions = extract_lp_positions(&changes("pool_revoked_last_holder", 63_599_411));
    assert!(
        positions.iter().any(|p| p.account_id
            == "GBUOTXGY3Z4BSRNNSPD3CJSOMGGYTCZALVLAMGH5UB2INVKVS46F5DQ7"
            && p.shares.parse::<f64>() == Ok(0.0)),
        "{positions:?}"
    );
}

#[test]
fn a_withdraw_then_exit_ends_at_zero() {
    let (_, snap) = pool(
        "pool_withdraw_then_exit",
        56_939_293,
        "59ef6a22d28b7122ec9f3d72393c6514c0497f2c40a5da297a356daccc22e74d",
    );
    assert_eq!(reserves(&snap), (0, 0, "0"));
}

#[test]
fn a_pool_seen_only_at_its_removal_keeps_its_row() {
    let (row, snap) = pool(
        "pool_removed_before_our_history",
        54_369_113,
        "c3f2b6ca1a2e7ac3b8fcec7765cbffd3a807448c5d3076d780480d5bd557fc2e",
    );
    assert_eq!(reserves(&snap), (0, 0, "0"));
    assert_eq!(row.last_updated_ledger, 54_369_113);
    assert_eq!(
        (row.asset_a["code"].as_str(), row.asset_b["code"].as_str()),
        (Some("AFR"), Some("ASVC"))
    );
    assert_eq!(row.fee_bps, 30);
}

#[test]
fn a_trustline_count_change_keeps_the_updated_reserves() {
    let (_, snap) = pool(
        "pool_trustline_count_only",
        64_437_548,
        "33a10b34f017f3367aa6c02a5df47f250605e8faf0cdc68ebe4dcd200d5d1f22",
    );
    assert_eq!(
        reserves(&snap),
        (6_080_679_402, 2_676_981_523_779_337, "3222235967698")
    );
}
