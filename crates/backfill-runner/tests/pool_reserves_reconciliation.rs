//! Task 0374 — every Soroban pool's newest `pool_state_changes` row equals the
//! reserves the pool holds in its own storage, read from the network.
//!
//! For each registered pool (`liquidity_pools`, `pool_kind = 1`):
//!
//! ```text
//! argMax(reserves, ledger_sequence) of pool_state_changes
//!     == the pool's own reserve entries read with `getLedgerEntries`
//! ```
//!
//! bit-exact, per family:
//!
//! - router (`pool_type_raw` a type name): the instance's `ReserveA`/`ReserveB`,
//!   `Reserves` or `Reserve0`/`Reserve1`, raw u128;
//! - pair-factory (`''`): the instance's u32 keys `2`/`3`, i128;
//! - config-factory (a digit): the persistent entries `u32(1)`/`u32(2)`, i128.
//!
//! The chain values are decoded here from XDR, not by the parser under test, so
//! a parser bug and a table bug both surface. This is the check that found the
//! plane's `Reserves × PrecisionMul` and a Phoenix-shaped pool whose code was
//! replaced; the ingest-time `error!` lines only see failures that announce
//! themselves.
//!
//! **Consistency.** The network is read first; ClickHouse is then bounded at the
//! highest ledger the RPC answered at, after waiting for the index to reach it. A
//! pool whose newest row is above the batch that read it moved during the run
//! and is reported as such, not as a mismatch. An entry the RPC does not return
//! (archived) cannot be compared and is counted, not failed.
//!
//! Gated on a client certificate for the production ClickHouse (read-only),
//! found where `api --bin local` finds it — `infra-hetzner/ca/out/$USER/`
//! (`$USER.crt` + `$USER.key`, git-ignored) — or at absolute
//! `POOL_CH_CERT`/`POOL_CH_KEY`. Skips when neither exists, so `cargo test`
//! stays offline. A few seconds.
//!
//! ```bash
//! cargo test -p backfill-runner --test pool_reserves_reconciliation -- --nocapture
//! ```
//!
//! Optional: `POOL_CH_DOMAIN` (default `ch.sorobanscan.rumblefish.dev`),
//! `POOL_RPC_URL` (default a public mainnet Soroban RPC).

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use stellar_xdr::{
    ContractDataDurability, LedgerEntryData, LedgerKey, LedgerKeyContractData, Limits, ReadXdr,
    ScAddress, ScSymbol, ScVal, ScVec, WriteXdr,
};

const RPC_BATCH: usize = 200;

struct Read {
    data: LedgerEntryData,
    answered_at: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Family {
    Router,
    Pair,
    Config,
}

#[tokio::test(flavor = "multi_thread")]
async fn every_pool_matches_its_own_storage() {
    let user = std::env::var("USER").unwrap_or_default();
    let local = format!(
        "{}/../../infra-hetzner/ca/out/{user}/{user}",
        env!("CARGO_MANIFEST_DIR")
    );
    let cert = std::env::var("POOL_CH_CERT").unwrap_or_else(|_| format!("{local}.crt"));
    let key = std::env::var("POOL_CH_KEY").unwrap_or_else(|_| format!("{local}.key"));
    if !std::path::Path::new(&cert).exists() || !std::path::Path::new(&key).exists() {
        eprintln!("no client certificate at {cert} — skipping the pool reserves reconciliation");
        return;
    }
    let domain = env_or("POOL_CH_DOMAIN", "ch.sorobanscan.rumblefish.dev");
    let rpc_url = env_or(
        "POOL_RPC_URL",
        "https://soroban-rpc.mainnet.stellar.gateway.fm/",
    );
    let bundle = db_clickhouse::mtls::MtlsBundle {
        cert_pem: std::fs::read_to_string(&cert).expect("read POOL_CH_CERT"),
        key_pem: std::fs::read_to_string(&key).expect("read POOL_CH_KEY"),
        ca_pem: String::new(),
    };
    let ch = db_clickhouse::mtls::client_with_mtls(&domain, &bundle, db_clickhouse::PROD_DATABASE)
        .expect("mTLS ClickHouse client");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("soroban-block-explorer/pool-reserves-reconciliation")
        .build()
        .unwrap();

    // 0. The registry: every Soroban pool and its family.
    let pools: Vec<(String, Family)> = ch
        .query(
            "SELECT hex(pool_id), any(pool_type_raw) FROM liquidity_pools \
             WHERE pool_kind = 1 GROUP BY pool_id",
        )
        .fetch_all::<(String, String)>()
        .await
        .unwrap()
        .into_iter()
        .map(|(hex_id, raw)| {
            let family = match raw.as_str() {
                "" => Family::Pair,
                r if r.bytes().all(|b| b.is_ascii_digit()) => Family::Config,
                _ => Family::Router,
            };
            (strkey(&hex_id), family)
        })
        .collect();
    assert!(!pools.is_empty(), "no Soroban pools registered");

    // 1. The network first.
    let keys: Vec<LedgerKey> = pools
        .iter()
        .flat_map(|(c, f)| match f {
            Family::Config => vec![
                contract_key(c, ScVal::U32(1)),
                contract_key(c, ScVal::U32(2)),
            ],
            _ => vec![contract_key(c, ScVal::LedgerKeyContractInstance)],
        })
        .collect();
    let reads = get_ledger_entries(&http, &rpc_url, &keys).await;
    let bound = reads
        .values()
        .map(|r| r.answered_at)
        .max()
        .expect("RPC answered");

    // 2. Wait for the index to reach the ledger the network answered at.
    let mut tip = 0u32;
    for _ in 0..30 {
        tip = ch
            .query("SELECT toUInt32(max(sequence)) FROM ledgers")
            .fetch_one::<u32>()
            .await
            .unwrap();
        if tip >= bound {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    assert!(
        tip >= bound,
        "index tip {tip} never reached the RPC ledger {bound}"
    );

    // 3. Our newest row per pool, bounded at the same ledger.
    let ours: HashMap<String, (u32, String)> = ch
        .query(&format!(
            "SELECT hex(pool_id), toUInt32(max(ledger_sequence)), \
             arrayStringConcat(arrayMap(x -> toString(x), argMax(reserves, ledger_sequence)), ',') \
             FROM pool_state_changes \
             WHERE pool_id IN (SELECT pool_id FROM liquidity_pools WHERE pool_kind = 1) \
               AND ledger_sequence <= {bound} \
             GROUP BY pool_id"
        ))
        .fetch_all::<(String, u32, String)>()
        .await
        .unwrap()
        .into_iter()
        .map(|(hex_id, l, r)| (strkey(&hex_id), (l, r)))
        .collect();

    // 4. Compare.
    let (mut equal, mut archived) = (0, 0);
    let (mut moved, mut failures) = (Vec::new(), Vec::new());
    for (pool, family) in &pools {
        let entries: Vec<Option<&Read>> = match family {
            Family::Config => [ScVal::U32(1), ScVal::U32(2)]
                .into_iter()
                .map(|k| reads.get(&key_b64(&contract_key(pool, k))))
                .collect(),
            _ => vec![reads.get(&key_b64(&contract_key(
                pool,
                ScVal::LedgerKeyContractInstance,
            )))],
        };
        if entries.iter().any(Option::is_none) {
            archived += 1;
            continue;
        }
        let entries: Vec<&Read> = entries.into_iter().flatten().collect();
        let Some(chain) = chain_reserves(*family, &entries) else {
            failures.push(format!("{pool}: no reserve entry this check can read"));
            continue;
        };
        let read_at = entries.iter().map(|r| r.answered_at).min().unwrap();
        let chain = chain.join(",");
        match ours.get(pool) {
            None if chain.split(',').all(|r| r == "0") => equal += 1,
            None => failures.push(format!("{pool}: no row of ours, chain holds [{chain}]")),
            Some((_, r)) if *r == chain => equal += 1,
            Some((l, r)) => {
                let msg = format!("{pool}: ours [{r}] at {l}, chain [{chain}]");
                if *l > read_at {
                    moved.push(msg)
                } else {
                    failures.push(msg)
                }
            }
        }
    }
    eprintln!(
        "\npool reserves reconciliation at RPC ledger {bound}: {} pools, {equal} equal, \
         {archived} not returned by the RPC, {} moved during the run, {} failures",
        pools.len(),
        moved.len(),
        failures.len()
    );
    for m in &moved {
        eprintln!("  ~ {m}");
    }
    for f in &failures {
        eprintln!("  ✗ {f}");
    }
    assert!(
        moved.is_empty(),
        "{} pools moved during the run — re-run it",
        moved.len()
    );
    assert!(
        failures.is_empty(),
        "{} failures — a defect to classify, never a tolerance",
        failures.len()
    );
}

/// The pool's reserves as the chain holds them, in leg order, as decimal
/// strings. `None` when no known layout is present — the pool's code stores
/// reserves somewhere this check does not know, which is itself a finding.
fn chain_reserves(family: Family, entries: &[&Read]) -> Option<Vec<String>> {
    let val = |r: &Read| match &r.data {
        LedgerEntryData::ContractData(cd) => Some(cd.val.clone()),
        _ => None,
    };
    let num = |v: &ScVal| match v {
        ScVal::U128(p) => Some(((u128::from(p.hi) << 64) | u128::from(p.lo)).to_string()),
        ScVal::I128(p) => Some(((i128::from(p.hi) << 64) | i128::from(p.lo)).to_string()),
        _ => None,
    };
    if family == Family::Config {
        return entries.iter().map(|r| num(&val(r)?)).collect();
    }
    let ScVal::ContractInstance(instance) = val(entries[0])? else {
        return None;
    };
    let storage = instance.storage?;
    let get = |k: &ScVal| storage.iter().find(|e| e.key == *k).map(|e| &e.val);
    let pair = |a: ScVal, b: ScVal| Some(vec![num(get(&a)?)?, num(get(&b)?)?]);
    match family {
        Family::Pair => pair(ScVal::U32(2), ScVal::U32(3)),
        _ => pair(sym_key("ReserveA"), sym_key("ReserveB"))
            .or_else(|| match get(&sym_key("Reserves"))? {
                ScVal::Vec(Some(legs)) => legs.iter().map(num).collect(),
                _ => None,
            })
            .or_else(|| pair(sym_key("Reserve0"), sym_key("Reserve1"))),
    }
}

/// The router family's instance keys are one-symbol vectors (`[Symbol(name)]`).
fn sym_key(name: &str) -> ScVal {
    ScVal::Vec(Some(
        ScVec::try_from(vec![ScVal::Symbol(ScSymbol::try_from(name).unwrap())]).unwrap(),
    ))
}

/// `pool_id` (hex of the 32-byte contract payload) → `C…` StrKey.
fn strkey(hex_id: &str) -> String {
    let bytes: [u8; 32] = hex::decode(hex_id).unwrap().try_into().unwrap();
    stellar_strkey::Contract(bytes)
        .to_string()
        .as_str()
        .to_string()
}

fn contract_key(contract: &str, key: ScVal) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::from_str(contract).unwrap(),
        key,
        durability: ContractDataDurability::Persistent,
    })
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn key_b64(key: &LedgerKey) -> String {
    BASE64.encode(key.to_xdr(Limits::none()).unwrap())
}

async fn get_ledger_entries(
    http: &reqwest::Client,
    url: &str,
    keys: &[LedgerKey],
) -> HashMap<String, Read> {
    let mut out = HashMap::new();
    for chunk in keys.chunks(RPC_BATCH) {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "getLedgerEntries",
            "params": { "keys": chunk.iter().map(key_b64).collect::<Vec<_>>() },
        });
        let resp: serde_json::Value = http
            .post(url)
            .json(&body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let result = resp
            .get("result")
            .unwrap_or_else(|| panic!("RPC error: {resp}"));
        let answered_at = result["latestLedger"].as_u64().expect("latestLedger") as u32;
        for e in result["entries"].as_array().into_iter().flatten() {
            let bytes = BASE64.decode(e["xdr"].as_str().unwrap()).unwrap();
            out.insert(
                e["key"].as_str().unwrap().to_string(),
                Read {
                    data: LedgerEntryData::from_xdr(bytes, Limits::none()).unwrap(),
                    answered_at,
                },
            );
        }
    }
    out
}
