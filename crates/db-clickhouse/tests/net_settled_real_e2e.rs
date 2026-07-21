//! E2E on real mainnet data (task 0393): run the PRODUCTION resolver
//! `classic_deltas_net_settled` with a real `sac_classic` registry over an AMM-swap
//! meta, proving that the contract-held SAC legs (`SacWrapped`) MERGE with the
//! account-side legs onto ONE `asset_id` each — the net-as-one property Horizon's
//! protocol-23 contract-effects showed, now proven through our actual production
//! path (registry reverse + reducer), not just the reader + reasoning.
//!
//! Fixture: `../xdr-parser/tests/fixtures/corpus/amm_swap_sac.b64`
//! (tx 3693dee4…, cross-validated 1:1 vs Horizon /effects on 2026-07-20).
//! Gated on fixture presence.

use std::collections::HashMap;

use base64::Engine;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::stage::classic_deltas_net_settled;
use stellar_xdr::{Limits, ReadXdr, TransactionMeta};
use xdr_parser::classic_balance_deltas;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../xdr-parser/tests/fixtures/corpus/amm_swap_sac.b64"
);
const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

#[test]
fn amm_swap_sac_and_account_legs_merge_to_one_asset_through_prod_resolver() {
    let Ok(b64) = std::fs::read_to_string(FIXTURE) else {
        eprintln!("skip: fixture absent (RPC retention ~1 week)");
        return;
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .expect("base64");
    let meta = TransactionMeta::from_xdr(bytes, Limits::none()).expect("decode meta");
    let deltas = classic_balance_deltas(&meta);
    // The reader emits 4 legs: account Native + Credit(USDC), contract SacWrapped(XLM),
    // SacWrapped(USDC). (Cross-checked 1:1 vs Horizon + stellar CLI.)
    assert_eq!(deltas.len(), 4, "expected the 4 swap legs, got {deltas:#?}");

    // Real registry: each SAC contract reverses to the wrapped classic's asset_id —
    // the SAME id the account-side Native / Credit legs resolve to.
    let native_id = ids::NATIVE_ASSET_ID;
    let usdc_id = ids::credit_asset_id("USDC", USDC_ISSUER);
    let sac_classic: HashMap<i64, i64> = [
        (ids::contract_id(XLM_SAC), native_id),
        (ids::contract_id(USDC_SAC), usdc_id),
    ]
    .into_iter()
    .collect();

    let ns = classic_deltas_net_settled(&deltas, &sac_classic);
    let by: HashMap<i64, Option<i128>> = ns.iter().map(|n| (n.asset_id, n.amount)).collect();

    // The 4 legs collapse to EXACTLY 2 assets: SacWrapped(XLM) + account Native → one
    // native row; SacWrapped(USDC) + account Credit → one USDC row. If the registry
    // reverse or the reducer were wrong, we'd get 3–4 rows (double-count) or wrong ids.
    assert_eq!(
        ns.len(),
        2,
        "SAC legs must merge with account legs → 2 assets, got {ns:#?}"
    );
    assert_eq!(
        by.get(&native_id),
        Some(&Some(5_001_793_319)),
        "native nets to the swap amount, not double: {ns:#?}"
    );
    assert_eq!(
        by.get(&usdc_id),
        Some(&Some(924_000_001)),
        "USDC nets to the swap amount, not double: {ns:#?}"
    );
}
