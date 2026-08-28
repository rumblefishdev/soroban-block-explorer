//! One-off generator for the step-4 registry backfill (task 0374).
//!
//! Reads the harvested `add_pool` corpus (chq JSONEachRow, see
//! `pool_router_real_corpus.rs` for the harvest command) and prints the
//! `INSERT INTO liquidity_pools` an operator runs once.
//!
//! A generator, not a writer, on purpose: the surrogate ids are the lower 64
//! bits of CityHash 1.0.2-128 — NOT ClickHouse's `cityHash64` — so SQL cannot
//! derive them. This bin uses the same `ids::` functions and the same decoder
//! the live parser uses, so the backfilled rows and future live rows are
//! byte-identical by construction.
//!
//! ```sh
//! cargo run -p db-clickhouse --bin gen_pool_registry_backfill \
//!     /tmp/add_pool_corpus.jsonl > /tmp/pool_registry_backfill.sql
//! ```

use xdr_parser::pool_router::parse_add_pool;

#[derive(serde::Deserialize)]
struct Row {
    ledger_sequence: i64,
    router: String,
    topics_xdr: String,
    data_xdr: String,
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: <corpus.jsonl>");
    let raw = std::fs::read_to_string(&path).expect("corpus readable");

    let mut values = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("corpus row");
        let topics: serde_json::Value = serde_json::from_str(&row.topics_xdr).expect("topics");
        let data: serde_json::Value = serde_json::from_str(&row.data_xdr).expect("data");
        // The corpus rows lack the emitter address column? They must carry it —
        // fail loudly rather than guess.
        let reg = parse_add_pool(&topics, &data).expect("corpus rows are all registrations");
        let pool_payload = contract_payload(&reg.pool).expect("pool strkey");
        let fee_bps: i32 = reg
            .init_args
            .first()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let legs: Vec<String> = reg
            .tokens
            .iter()
            .map(|t| db_clickhouse::persist::ids::contract_id(t).to_string())
            .collect();
        values.push(format!(
            "(unhex('{}'),0,'',0,0,'',0,{},{},1,[{}],{},'{}',0)",
            hex::encode(pool_payload),
            fee_bps,
            row.ledger_sequence,
            legs.join(","),
            db_clickhouse::persist::ids::contract_id(&row.router),
            reg.pool_type.replace('\'', ""),
        ));
    }
    println!(
        "INSERT INTO liquidity_pools (pool_id, asset_a_type, asset_a_code, asset_a_issuer_id, \
         asset_b_type, asset_b_code, asset_b_issuer_id, fee_bps, last_updated_ledger, pool_kind, \
         legs, deployment_id, pool_type_raw, share_token_id) VALUES"
    );
    println!("{};", values.join(",\n"));
}

fn contract_payload(strkey: &str) -> Option<[u8; 32]> {
    match stellar_strkey::Strkey::from_string(strkey) {
        Ok(stellar_strkey::Strkey::Contract(c)) => Some(c.0),
        _ => None,
    }
}
