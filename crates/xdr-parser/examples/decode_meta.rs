//! Dev harness (task 0393 cross-validation): decode a base64 `TransactionMeta`
//! and print OUR ledger-value output — per-(account, asset) signed deltas from
//! `ledger_balance_deltas`, and `net_settled = max(Σ+, Σ−)` per asset. Compared
//! by hand against Horizon `/effects` (independent decode) + stellar.expert.
//!
//! Usage: cargo run -q -p xdr-parser --example decode_meta -- <base64-meta>
use base64::Engine;
use std::collections::{BTreeMap, BTreeSet};
use stellar_xdr::{Limits, ReadXdr, TransactionMeta};
use xdr_parser::ledger_balance_deltas;

fn main() {
    let b64 = std::env::args()
        .nth(1)
        .expect("usage: decode_meta <b64-meta>");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .expect("base64");
    let meta = TransactionMeta::from_xdr(bytes, Limits::none()).expect("decode meta");
    let deltas = ledger_balance_deltas(&meta);

    println!("# per (account, asset) signed delta (raw stroops):");
    for d in &deltas {
        println!("  {:>22}  {:?}  {}", d.delta, d.asset, d.account);
    }

    // net_settled = max(Σ+, Σ−) per asset over the per-account net deltas
    // (ledger_balance_deltas already telescopes per account, so this is exactly
    // the reducer's definition).
    let mut pos: BTreeMap<String, i128> = BTreeMap::new();
    let mut neg: BTreeMap<String, i128> = BTreeMap::new();
    for d in &deltas {
        let k = format!("{:?}", d.asset);
        if d.delta >= 0 {
            *pos.entry(k).or_default() += d.delta;
        } else {
            *neg.entry(k).or_default() += -d.delta;
        }
    }
    println!("# net_settled per asset (max Σ+,Σ−):");
    let keys: BTreeSet<_> = pos.keys().chain(neg.keys()).cloned().collect();
    for k in keys {
        let p = pos.get(&k).copied().unwrap_or(0);
        let n = neg.get(&k).copied().unwrap_or(0);
        println!("  {:>22}   {}", p.max(n), k);
    }
}
