//! Task 0540 — rollout gate 7c (map ticket T11): an account's transfers sum to
//! its balance.
//!
//! For each listed account `A` and every asset `X` it ever moved:
//!
//! ```text
//! Σ asset_transfers(to = A, X) − Σ asset_transfers(from = A, X) − fee_leg(A, X)
//!     == balance of (A, X) read from the network with `getLedgerEntries`
//! ```
//!
//! bit-exact, where the fee leg is XLM only and comes from the protocol's `fee`
//! events (native SAC, topics `["fee", payer]`, refunds negative) — they name the
//! account that actually paid, a fee-bump sponsor included. `transactions`
//! stores the inner source, not the fee payer, so it cannot close an account that
//! mixes bumped and own transactions.
//!
//! Gate 7b proves the table equals a re-decode of the archive; this proves the
//! decoder right, against state no code of ours wrote. Accounts are listed rather
//! than sampled: each covers a different way value moves.
//!
//! **An account must be younger than the ingest floor in its FIRST incarnation.**
//! `seq_num >> 32` dates only the current one, and merge-then-recreate resets it.
//! So the edges and fees before the current creation ledger are summed
//! separately and must net to zero in every asset — an incarnation that was merged
//! away leaves nothing behind. A non-zero prior sum means history below the floor
//! that the window cannot hold; the test fails and names the account rather than
//! reporting a mismatch. An account merged away and never re-created counts as
//! wholly prior.
//!
//! **Consistency.** The network is read first; ClickHouse is then bounded at the
//! highest ledger the RPC answered at, after waiting for the index to reach it.
//! Trustlines and token balances are read after that pass. A pair whose last edge
//! or fee is newer than the batch that read it, or whose entry was last written
//! above the bound, moved during the run and is reported as such, not as a
//! mismatch.
//!
//! **Cost.** One pass over `asset_transfers` in 100 000-ledger windows (a window
//! that fails is retried split into four), and a pass over the native SAC's `fee`
//! events restricted to the listed accounts' transactions — a scan by `topics_xdr`
//! alone exhausts the read-bytes quota. Expect tens of minutes; run it on
//! demand, not in CI.
//!
//! Gated on a client certificate for the production ClickHouse (read-only);
//! skips when unset, so `cargo test` stays offline. Paths must be absolute:
//! cargo runs integration tests from the package directory.
//!
//! ```bash
//! T11_CH_CERT=/abs/client.crt T11_CH_KEY=/abs/client.key \
//!     cargo test -p backfill-runner --test account_reconciliation -- --nocapture
//! ```
//!
//! Optional: `T11_CH_DOMAIN` (default `ch.sorobanscan.rumblefish.dev`),
//! `T11_RPC_URL` (default a public mainnet Soroban RPC), `T11_ACCOUNTS`
//! (comma-separated G-StrKeys replacing the list below).

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use stellar_xdr::{
    AccountId, AlphaNum4, AlphaNum12, AssetCode4, AssetCode12, ContractDataDurability,
    LedgerEntryData, LedgerKey, LedgerKeyAccount, LedgerKeyContractData, LedgerKeyTrustLine,
    Limits, ReadXdr, ScAddress, ScSymbol, ScVal, ScVec, TrustLineAsset, WriteXdr,
};

/// The accounts of the 2026-09-13 run (0540 README, "Completion gate 7 passed on
/// the full range"), minus `GAON23LEC5MNOXF6GPNGKZT2456H6KKTJZS3SW22NE2AO3Q6AOKXBDXF`,
/// whose first incarnation predates the floor — the guard above exists because
/// of it.
const ACCOUNTS: &[&str] = &[
    // merged; every tx fee-bumped
    "GDYYRX3NVXB3WU4CRJIFRHUAYYDJVK572NCIMXUVF52SQFLLLZSU53BX",
    "GCDDDAHALHBVITLP42KS5M5RON2MEDC6AHOULSC6EEU27UMJSHZSFJYU",
    // created and merged six times
    "GDKFRCA66NQSDODH4HK2BHENYKJKVV33CRP33EDOMCGGMZIZLS7B7I2W",
    // clawback subject
    "GAQUUJPPZNSV5TET6DG2I73UEF7GKJP5LGLPABQQSFQVKGPTCX6OEMNA",
    "GADEE6SN7XOU732RNIDNP4BZH4FG2MOME2DWMA7UYKBOQUMXLJFXTYCN",
    "GBVJNKNPR3PMURROXMKLM3ZKTKISS3FYM3NNS26KHU4UVHO37W5FDKJ4",
    // claimable balances; ~186 M edges
    "GD26W2HVRM7DS7VVAMPZVMV7WEFP7SLWQLPUNX7SFM525MIS223WCEUB",
    // claimable balances
    "GDYPQUMNWTYVTZJIN2FYMKBSEHUSMKU7H2254JPRCTRUQ2SW3NQX3SPC",
    "GDP73K5XSZ7B6MPQZBHKVTBGU2QDMMUQUPBWE57ATJZJRIBHEI7CVWQG",
    // classic pools; mixed fee payers
    "GAV4FNC74K2SD3TYZDOXSYHAC5FI3LZ53LHLSWHH4OPS32L3T6LXMSLP",
    // classic pools; Soroban refunds
    "GA3VTARHH7IGI4UDFZBV3N7ZPCSS3NWYYXY42F3R4MV5ABITVGY2TKFW",
    // contract counterparty
    "GAUAICHEQTIKMN2D3KKRSQMB4NRJEFLWD32TBJZXKLQQYCGWNL7B7T3Q",
    "GAEYMOFVVKQ2ALN573SRQY6FERDSBCBT3MCMSSIXN2UGNRIAKTKWYB7N",
    "GA3ECAIHY5EZXSO6NI2FLYE3HLKGH3OFVN56PAYR37HMM5CZBIEG4GMJ",
    // muxed destination
    "GAT52S3LSPWEYTZVGE3G7NDZALJX4KELKTO4JSZLQFNBURWLUEYGANS3",
    // bespoke token holder
    "GCQM5CVSGJPFNJGN5ZUPFI5PSBI2KU6Z7AAUPGRQO6BLYALAQ7KBCM3J",
    "GCKCNT4TXHISNNBIPUVOJ6VJYKPJYNJLF2IIP2P3EIZIUBSQYSRRY37U",
    "GAFZO5Q7EHV3P4L6MLDJIHD2OLMN5DL4PSEH4MCX5EOVAJSZYUR2FUVD",
    "GDZMVZ3TC7KHOUXAIRXFGRRYTIJJBYLO2GTRTCWKV3OZUUL4LXSVBJZA",
];

/// `intDiv(ledger_sequence, 500000)` of the ingest floor, 50 457 424.
const FIRST_PARTITION: u32 = 100;
const PARTITION: u32 = 500_000;
const WINDOW: u32 = 100_000;
/// The native SAC's surrogate — the `contract_id` every `fee` event carries.
const NATIVE_SAC_ID: i64 = -6_164_601_581_949_826_601;
/// `getLedgerEntries` accepts up to 200 keys per call.
const RPC_BATCH: usize = 200;

#[derive(clickhouse::Row, serde::Deserialize)]
struct EdgeSum {
    acc: i64,
    asset_id: i64,
    prior: u8,
    delta: i128,
    n: u64,
    last: i64,
}

#[derive(clickhouse::Row, serde::Deserialize)]
struct FeeSum {
    g: String,
    prior: u8,
    net: i128,
    n: u64,
    last: i64,
}

#[derive(clickhouse::Row, serde::Deserialize)]
struct AssetRow {
    id: i64,
    asset_type: i16,
    asset_code: String,
    issuer: String,
    contract: String,
}

/// One entry read from the network, with the ledger its batch was answered at.
struct Read {
    data: LedgerEntryData,
    answered_at: u32,
    modified: u32,
}

#[derive(Default)]
struct Sums {
    current: i128,
    prior: i128,
    n: u64,
    last: i64,
}

#[tokio::test(flavor = "multi_thread")]
async fn every_account_sums_to_its_ledger_balance() {
    let (Some(cert), Some(key)) = (
        std::env::var_os("T11_CH_CERT"),
        std::env::var_os("T11_CH_KEY"),
    ) else {
        eprintln!("T11_CH_CERT / T11_CH_KEY not set — skipping the account reconciliation");
        return;
    };
    let domain = env_or("T11_CH_DOMAIN", "ch.sorobanscan.rumblefish.dev");
    let rpc_url = env_or(
        "T11_RPC_URL",
        "https://soroban-rpc.mainnet.stellar.gateway.fm/",
    );
    let accounts: Vec<String> = match std::env::var("T11_ACCOUNTS") {
        Ok(list) => list.split(',').map(|s| s.trim().to_string()).collect(),
        Err(_) => ACCOUNTS.iter().map(|g| g.to_string()).collect(),
    };
    for g in &accounts {
        AccountId::from_str(g).unwrap_or_else(|e| panic!("{g} is not a G-StrKey: {e}"));
    }

    let bundle = db_clickhouse::mtls::MtlsBundle {
        cert_pem: std::fs::read_to_string(&cert).expect("read T11_CH_CERT"),
        key_pem: std::fs::read_to_string(&key).expect("read T11_CH_KEY"),
        ca_pem: String::new(),
    };
    let ch = db_clickhouse::mtls::client_with_mtls(&domain, &bundle, db_clickhouse::PROD_DATABASE)
        .expect("mTLS ClickHouse client");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("soroban-block-explorer/account-reconciliation")
        .build()
        .unwrap();

    // 1. The network first: account entries give the balance and the current
    //    incarnation's creation ledger.
    let account_keys: Vec<LedgerKey> = accounts
        .iter()
        .map(|g| {
            LedgerKey::Account(LedgerKeyAccount {
                account_id: AccountId::from_str(g).unwrap(),
            })
        })
        .collect();
    let account_reads = get_ledger_entries(&http, &rpc_url, &account_keys).await;
    let mut born: HashMap<String, i64> = HashMap::new();
    for g in &accounts {
        let k = key_b64(&LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId::from_str(g).unwrap(),
        }));
        born.insert(
            g.clone(),
            match account_reads.get(&k).map(|r| &r.data) {
                Some(LedgerEntryData::Account(a)) => a.seq_num.0 >> 32,
                // Merged away and not re-created: everything is a past incarnation.
                _ => i64::MAX,
            },
        );
    }
    let bound = account_reads
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

    let ids: HashMap<String, i64> = ch
        .query(&format!(
            "SELECT account_id, any(id) FROM accounts WHERE account_id IN ({}) GROUP BY account_id",
            quoted(&accounts)
        ))
        .fetch_all::<(String, i64)>()
        .await
        .unwrap()
        .into_iter()
        .collect();
    let unknown: Vec<_> = accounts.iter().filter(|g| !ids.contains_key(*g)).collect();
    assert!(
        unknown.is_empty(),
        "accounts the index does not know: {unknown:?}"
    );
    let g_of: HashMap<i64, String> = ids.iter().map(|(g, id)| (*id, g.clone())).collect();
    let id_list = ids
        .values()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let born_by_id = ids
        .iter()
        .map(|(g, id)| (id.to_string(), born[g].to_string()))
        .collect::<Vec<_>>();
    let (born_ids, born_vals): (Vec<_>, Vec<_>) = born_by_id.into_iter().unzip();
    let born_g = accounts
        .iter()
        .map(|g| born[g].to_string())
        .collect::<Vec<_>>();

    // 3. Edges and fees, bounded at `bound`, split at each account's creation.
    let mut sums: BTreeMap<(String, i64), Sums> = BTreeMap::new();
    let mut fees: HashMap<String, Sums> = HashMap::new();
    let mut edge_count = 0u64;
    for p in FIRST_PARTITION..=bound / PARTITION {
        for w in (0..PARTITION).step_by(WINDOW as usize) {
            let lo = p * PARTITION + w;
            if lo > bound {
                break;
            }
            let hi = (lo + WINDOW - 1).min(bound);
            let edge_sql = |lo: u32, hi: u32| {
                format!(
                    "SELECT side.1 AS acc, asset_id, \
                            toUInt8(ledger_sequence < transform(side.1, CAST([{bi}] AS Array(Int64)), \
                                                                CAST([{bv}] AS Array(Int64)), toInt64(0))) AS prior, \
                            sum(side.2) AS delta, count() AS n, max(ledger_sequence) AS last \
                     FROM (SELECT ledger_sequence, asset_id, assumeNotNull(amount) AS amt, from_id, to_id \
                           FROM asset_transfers FINAL \
                           WHERE ledger_sequence BETWEEN {lo} AND {hi} AND amount IS NOT NULL \
                             AND (from_id IN ({id_list}) OR to_id IN ({id_list}))) \
                     ARRAY JOIN [tuple(ifNull(to_id, 0), amt), tuple(ifNull(from_id, 0), -amt)] AS side \
                     WHERE side.1 IN ({id_list}) \
                     GROUP BY acc, asset_id, prior",
                    bi = born_ids.join(","),
                    bv = born_vals.join(","),
                )
            };
            for r in fetch_windowed::<EdgeSum>(&ch, lo, hi, &edge_sql).await {
                let s = sums.entry((g_of[&r.acc].clone(), r.asset_id)).or_default();
                if r.prior == 1 {
                    s.prior += r.delta
                } else {
                    s.current += r.delta
                }
                s.n += r.n;
                s.last = s.last.max(r.last);
                edge_count += r.n;
            }
            let fee_sql = |lo: u32, hi: u32| {
                format!(
                    "SELECT g, toUInt8(ledger_sequence < transform(g, [{gs}], CAST([{bv}] AS Array(Int64)), \
                                                                   toInt64(0))) AS prior, \
                            sum(fee) AS net, count() AS n, max(ledger_sequence) AS last \
                     FROM (SELECT ledger_sequence, transaction_id, event_index, \
                                  JSONExtractString(topics_xdr, 2, 'value') AS g, \
                                  toInt128(JSONExtractString(data_xdr, 'value')) AS fee \
                           FROM soroban_events \
                           WHERE contract_id = {NATIVE_SAC_ID} AND ledger_sequence BETWEEN {lo} AND {hi} \
                             AND signature = 'fee' \
                             AND transaction_id IN (SELECT transaction_id FROM transaction_participants \
                                                    WHERE account_id IN ({id_list}) \
                                                      AND ledger_sequence BETWEEN {lo} AND {hi}) \
                           LIMIT 1 BY transaction_id, event_index) \
                     WHERE g IN ({gs}) \
                     GROUP BY g, prior",
                    gs = quoted(&accounts),
                    bv = born_g.join(","),
                )
            };
            for r in fetch_windowed::<FeeSum>(&ch, lo, hi, &fee_sql).await {
                let s = fees.entry(r.g).or_default();
                if r.prior == 1 {
                    s.prior += r.net
                } else {
                    s.current += r.net
                }
                s.n += r.n;
                s.last = s.last.max(r.last);
            }
        }
        eprintln!("  partition {p} done ({edge_count} edges so far)");
    }

    // 4. What each asset id is, and the ledger key that holds its balance.
    let native_id: i64 = ch
        .query("SELECT any(id) FROM assets WHERE asset_type = 0")
        .fetch_one()
        .await
        .unwrap();
    for g in &accounts {
        let s = sums.entry((g.clone(), native_id)).or_default();
        if let Some(f) = fees.get(g) {
            s.current -= f.current;
            s.prior -= f.prior;
            s.last = s.last.max(f.last);
        }
    }
    let asset_ids = sums
        .keys()
        .map(|(_, a)| a.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let asset_list = asset_ids.into_iter().collect::<Vec<_>>().join(",");
    let assets: HashMap<i64, AssetRow> = ch
        .query(&format!(
            // No `join_use_nulls`: the read-only user cannot change settings, so an
            // unmatched join leaves column defaults — `found` tells a missing
            // `assets` row apart from a real `asset_type = 0` (native).
            "SELECT ids.id AS id, if(a.found = 1, a.asset_type, toInt16(-1)) AS asset_type, \
                    a.asset_code AS asset_code, iss.account_id AS issuer, sc.contract_id AS contract \
             FROM (SELECT arrayJoin(CAST([{asset_list}] AS Array(Int64))) AS id) ids \
             LEFT JOIN (SELECT id, toUInt8(1) AS found, any(asset_type) AS asset_type, \
                               any(asset_code) AS asset_code, any(issuer_id) AS issuer_id \
                        FROM assets WHERE id IN ({asset_list}) GROUP BY id) a \
               ON a.id = ids.id \
             LEFT JOIN (SELECT id, any(account_id) AS account_id FROM accounts \
                        WHERE id IN (SELECT issuer_id FROM assets WHERE id IN ({asset_list})) GROUP BY id) iss \
               ON iss.id = a.issuer_id \
             LEFT JOIN (SELECT id, any(contract_id) AS contract_id FROM soroban_contracts \
                        WHERE id IN ({asset_list}) GROUP BY id) sc \
               ON sc.id = ids.id"
        ))
        .fetch_all::<AssetRow>()
        .await
        .unwrap()
        .into_iter()
        .map(|a| (a.id, a))
        .collect();

    let mut keyed: Vec<((String, i64), Option<LedgerKey>)> = Vec::new();
    let mut unresolved = Vec::new();
    for (g, asset) in sums.keys() {
        let a = &assets[asset];
        let key = match a.asset_type {
            0 => Some(LedgerKey::Account(LedgerKeyAccount {
                account_id: AccountId::from_str(g).unwrap(),
            })),
            // The issuer holds no trustline to its own asset; nothing to read.
            1 | 2 if a.issuer == *g => None,
            1 | 2 if !a.issuer.is_empty() => Some(trustline_key(g, &a.asset_code, &a.issuer)),
            _ if !a.contract.is_empty() => Some(balance_key(&a.contract, g)),
            _ => {
                unresolved.push(format!("{g} asset {asset} (type {})", a.asset_type));
                continue;
            }
        };
        keyed.push(((g.clone(), *asset), key));
    }
    assert!(
        unresolved.is_empty(),
        "assets with no ledger key to read: {unresolved:#?}"
    );

    let wanted: Vec<LedgerKey> = keyed.iter().filter_map(|(_, k)| k.clone()).collect();
    let mut reads = get_ledger_entries(&http, &rpc_url, &wanted).await;
    reads.extend(account_reads);

    // 5. Compare.
    let mut failures = Vec::new();
    let mut moved = Vec::new();
    let mut compared = 0usize;
    for g in &accounts {
        let mut line = format!(
            "{g}  born {}",
            if born[g] == i64::MAX {
                "merged".into()
            } else {
                born[g].to_string()
            }
        );
        let mut pairs = 0;
        for ((kg, asset), key) in keyed.iter().filter(|((kg, _), _)| kg == g) {
            let s = &sums[&(kg.clone(), *asset)];
            if s.prior != 0 {
                failures.push(format!(
                    "{g}: asset {asset} nets {} before the current creation ledger {} — history below the floor; \
                     remove the account from the list",
                    s.prior, born[g]
                ));
            }
            let Some(key) = key else { continue };
            let read = reads.get(&key_b64(key));
            let actual = match read.map(|r| &r.data) {
                None => 0,
                Some(LedgerEntryData::Account(a)) => i128::from(a.balance),
                Some(LedgerEntryData::Trustline(t)) => i128::from(t.balance),
                Some(LedgerEntryData::ContractData(cd)) => match &cd.val {
                    ScVal::I128(p) => (i128::from(p.hi) << 64) | i128::from(p.lo),
                    other => panic!("{g} asset {asset}: balance is not an i128: {other:?}"),
                },
                Some(other) => panic!("{g} asset {asset}: unexpected entry {}", other.name()),
            };
            // Moved during the run: an edge newer than the batch that read the
            // entry, or — for entries read after the index pass — an entry last
            // written above the bound the index was summed to.
            let moved_during_run =
                read.is_some_and(|r| s.last > i64::from(r.answered_at) || r.modified > bound);
            compared += 1;
            pairs += 1;
            if s.current != actual {
                let msg = format!(
                    "{g} asset {asset}: expected {} actual {actual} (diff {})",
                    s.current,
                    actual - s.current
                );
                if moved_during_run {
                    moved.push(msg)
                } else {
                    failures.push(msg)
                }
            }
        }
        line.push_str(&format!(
            "  pairs {pairs}  fee events {}",
            fees.get(g).map_or(0, |f| f.n)
        ));
        eprintln!("{line}");
    }
    eprintln!(
        "\naccount reconciliation at RPC ledger {bound}: {} accounts, {compared} (account, asset) pairs, \
         {edge_count} edges, {} moved during the run, {} failures",
        accounts.len(),
        moved.len(),
        failures.len()
    );
    for m in &moved {
        eprintln!("  ~ {m}");
    }
    for f in failures.iter().take(40) {
        eprintln!("  ✗ {f}");
    }
    assert!(
        moved.is_empty(),
        "{} pairs moved during the run — re-run it",
        moved.len()
    );
    assert!(
        failures.is_empty(),
        "{} failures — a defect to classify, never a tolerance",
        failures.len()
    );
    assert!(compared > 0, "compared nothing");
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Run a windowed statement; a window the server refuses (memory, time) is
/// split into four and retried once at that size before the test fails.
async fn fetch_windowed<T>(
    ch: &clickhouse::Client,
    lo: u32,
    hi: u32,
    sql: &dyn Fn(u32, u32) -> String,
) -> Vec<T>
where
    T: clickhouse::RowOwned + clickhouse::RowRead,
{
    match ch.query(&sql(lo, hi)).fetch_all::<T>().await {
        Ok(rows) => rows,
        Err(first) => {
            let step = (hi - lo + 1).div_ceil(4);
            let mut out = Vec::new();
            for s in (lo..=hi).step_by(step as usize) {
                let e = (s + step - 1).min(hi);
                out.extend(
                    ch.query(&sql(s, e))
                        .fetch_all::<T>()
                        .await
                        .unwrap_or_else(|e2| {
                            panic!("window {s}..{e} failed after split ({first}): {e2}")
                        }),
                );
            }
            out
        }
    }
}

fn key_b64(key: &LedgerKey) -> String {
    BASE64.encode(key.to_xdr(Limits::none()).unwrap())
}

fn trustline_key(g: &str, code: &str, issuer: &str) -> LedgerKey {
    let issuer = AccountId::from_str(issuer).unwrap();
    let asset = if code.len() <= 4 {
        TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4::from_str(code).unwrap(),
            issuer,
        })
    } else {
        TrustLineAsset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12::from_str(code).unwrap(),
            issuer,
        })
    };
    LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id: AccountId::from_str(g).unwrap(),
        asset,
    })
}

/// The persistent `Vec[Symbol("Balance"), Address(holder)]` entry — the layout the
/// ledger reader relies on; a token storing balances any other way fails loudly
/// at the `i128` match rather than reading as zero.
fn balance_key(contract: &str, holder: &str) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::from_str(contract).unwrap(),
        key: ScVal::Vec(Some(
            ScVec::try_from(vec![
                ScVal::Symbol(ScSymbol::try_from("Balance").unwrap()),
                ScVal::Address(ScAddress::from_str(holder).unwrap()),
            ])
            .unwrap(),
        )),
        durability: ContractDataDurability::Persistent,
    })
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
            let data = LedgerEntryData::from_xdr(bytes, Limits::none()).unwrap();
            let modified = e["lastModifiedLedgerSeq"]
                .as_u64()
                .expect("lastModifiedLedgerSeq") as u32;
            out.insert(
                e["key"].as_str().unwrap().to_string(),
                Read {
                    data,
                    answered_at,
                    modified,
                },
            );
        }
    }
    out
}
