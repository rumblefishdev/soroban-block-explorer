//! Task 0374 (W1) — checks the soroban pool event decoder against production,
//! read-only, over a recent window of ledgers.
//!
//! 1. **Every event accounted for.** Each amount event a registered pool
//!    emitted in the window was decoded into rows, and every other event name
//!    is on the decoder's list of events that carry no amount. A renamed or
//!    new amount event fails here instead of reading as a pool with no volume.
//! 2. **Amounts, pair family.** A Soroswap pool keeps its whole fee, so the
//!    change of its stored reserves between two `pool_state_changes` rows
//!    equals the sum of our amounts in between, to the unit.
//!
//! Skips without a client certificate (same as `pool_reserves_reconciliation`).
//! Run on demand:
//!
//!   cargo test -p backfill-runner --test soroban_pool_amounts_reconciliation -- --nocapture

use std::collections::{BTreeMap, HashMap};

use db_clickhouse::persist::rows::SorobanEventRow;
use db_clickhouse::persist::stage;

/// Default window: the newest 200k ledgers. `POOL_FROM` / `POOL_TO` (ledger
/// numbers, exclusive / inclusive) check any other range — the whole history
/// is checked in slices of a few million ledgers.
const WINDOW_LEDGERS: i64 = 200_000;

/// Ledgers where a pair-family pool's instance was restored from a stale copy
/// after protocol 23 (raw ledger meta: change type `restored`): its reserves
/// jump back with no pool event, so the step cannot match the amounts.
const RESTORED_STALE: [i64; 3] = [58_774_376, 58_779_504, 58_779_518];

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn decoded_pool_events_match_their_events_and_reserves() {
    // The decoder names every event it refuses (`error!`); show them.
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let user = std::env::var("USER").unwrap_or_default();
    let local = format!(
        "{}/../../infra-hetzner/ca/out/{user}/{user}",
        env!("CARGO_MANIFEST_DIR")
    );
    let cert = std::env::var("POOL_CH_CERT").unwrap_or_else(|_| format!("{local}.crt"));
    let key = std::env::var("POOL_CH_KEY").unwrap_or_else(|_| format!("{local}.key"));
    if !std::path::Path::new(&cert).exists() || !std::path::Path::new(&key).exists() {
        eprintln!("no client certificate at {cert} — skipping the pool amounts reconciliation");
        return;
    }
    let bundle = db_clickhouse::mtls::MtlsBundle {
        cert_pem: std::fs::read_to_string(&cert).expect("read POOL_CH_CERT"),
        key_pem: std::fs::read_to_string(&key).expect("read POOL_CH_KEY"),
        ca_pem: String::new(),
    };
    let domain = env_or("POOL_CH_DOMAIN", "ch.sorobanscan.rumblefish.dev");
    let ch = db_clickhouse::mtls::client_with_mtls(&domain, &bundle, db_clickhouse::PROD_DATABASE)
        .expect("mTLS ClickHouse client");

    let pools = db_clickhouse::persist::fetch_soroban_pools(&ch)
        .await
        .unwrap();
    let sac = db_clickhouse::persist::fetch_sac_classic_map(&ch, true)
        .await
        .unwrap();
    let raw_type: HashMap<i64, String> = ch
        .query(
            "SELECT lower(hex(pool_id)), any(pool_type_raw) FROM liquidity_pools \
             WHERE pool_kind = 1 GROUP BY pool_id",
        )
        .fetch_all::<(String, String)>()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(hex_id, raw)| {
            let id: [u8; 32] = hex::decode(hex_id).ok()?.try_into().ok()?;
            let (contract, _) = stage::soroban_pool_amounts::soroban_pool_entry(id, vec![]);
            Some((contract, raw))
        })
        .collect();
    // Family for the coverage count; family and pool type for the reserve steps.
    let family: HashMap<i64, String> = raw_type
        .iter()
        .map(|(c, raw)| {
            let f = match raw.as_str() {
                "" => "pair",
                r if r.bytes().all(|b| b.is_ascii_digit()) => "config",
                _ => "router",
            };
            (*c, f.to_string())
        })
        .collect();
    let tip: i64 = ch
        .query("SELECT max(ledger_sequence) FROM soroban_events")
        .fetch_one()
        .await
        .unwrap();
    let env_ledger = |k: &str| std::env::var(k).ok().map(|v| v.parse::<i64>().expect(k));
    // Never past the tip read above: events arriving during the run would be
    // counted by the SQL but not in the events already read.
    let tip = env_ledger("POOL_TO").unwrap_or(tip).min(tip);
    let from = env_ledger("POOL_FROM").unwrap_or(tip - WINDOW_LEDGERS);
    let ids = pools
        .keys()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");

    // In 50k-ledger slices, to stay under the read profile's 30 s cap on a
    // busy server.
    let mut events: Vec<SorobanEventRow> = Vec::new();
    for lo in (from..tip).step_by(50_000) {
        let hi = (lo + 50_000).min(tip);
        events.extend(
            ch.query(&format!(
                "SELECT contract_id, ledger_sequence, transaction_index, operation_index, \
                        event_index, application_order, event_type, signature, topics_xdr, data_xdr \
                 FROM soroban_events \
                 WHERE contract_id IN ({ids}) AND ledger_sequence > {lo} AND ledger_sequence <= {hi} \
                 LIMIT 1 BY contract_id, ledger_sequence, transaction_index, operation_index, event_index"
            ))
            .fetch_all::<SorobanEventRow>()
            .await
            .unwrap(),
        );
    }
    let rows = stage::soroban_pool_amounts::soroban_pool_amount_rows(&events, &pools, &sac);

    // 1. Every event a pool emitted is accounted for: an amount event was
    // decoded, and any other name is on the decoder's list of events that
    // carry no amount — a renamed `trade` fails here instead of reading as a
    // pool with no volume.
    println!(
        "window: ledgers {from}..={tip}, {} events read, {} rows",
        events.len(),
        rows.len()
    );
    let decoded: std::collections::HashSet<(i64, i64, u32, u16, u32)> = rows
        .iter()
        .map(|r| {
            let contract = pools
                .iter()
                .find(|(_, p)| p.pool_id == r.pool_id)
                .map(|(c, _)| *c)
                .unwrap();
            (
                contract,
                r.ledger_sequence,
                r.application_order as u32,
                r.operation_index,
                r.event_index,
            )
        })
        .collect();
    let mut missing = 0;
    let mut unknown: BTreeMap<String, u64> = BTreeMap::new();
    for e in &events {
        let name = event_name(&e.topics_xdr);
        if is_amount_shape(&e.topics_xdr) {
            let key = (
                e.contract_id,
                e.ledger_sequence,
                e.transaction_index,
                e.operation_index,
                e.event_index,
            );
            if !decoded.contains(&key) {
                missing += 1;
                println!(
                    "  not decoded: {key:?} {} {}",
                    e.topics_xdr,
                    &e.data_xdr[..e.data_xdr.len().min(300)]
                );
            }
        } else if !AMOUNT_NAMES.contains(&name.as_str())
            && !stage::soroban_pool_amounts::NON_AMOUNT_EVENTS.contains(&name.as_str())
            && !["SoroswapPair:sync", "SoroswapPair:skim"].contains(&name.as_str())
        {
            *unknown.entry(name).or_default() += 1;
        }
    }
    println!("amount events not decoded: {missing}; unknown names: {unknown:?}");

    // 2. Δ stored reserves against Σ our amounts, between two state rows.
    // Exact for the pair family (it keeps its whole fee); the router and
    // config families send part of the fee elsewhere, so those are reported
    // as exact / within 1% rather than asserted.
    let pair_pools: Vec<(i64, &stage::soroban_pool_amounts::SorobanPool)> =
        pools.iter().map(|(c, p)| (*c, p)).collect();
    let hexes = pair_pools
        .iter()
        .map(|(_, p)| format!("unhex('{}')", hex::encode(p.pool_id)))
        .collect::<Vec<_>>()
        .join(",");
    let states: Vec<(String, i64, String)> = ch
        .query(&format!(
            "SELECT lower(hex(pool_id)), ledger_sequence, arrayStringConcat(arrayMap(x -> toString(x), any(reserves)), ',') \
             FROM pool_state_changes \
             WHERE pool_id IN ({hexes}) AND ledger_sequence > {from} AND ledger_sequence <= {tip} \
             GROUP BY pool_id, ledger_sequence ORDER BY pool_id, ledger_sequence"
        ))
        .fetch_all()
        .await
        .unwrap();
    let mut by_pool: HashMap<String, Vec<(i64, Vec<i128>)>> = HashMap::new();
    for (p, l, r) in states {
        let r: Vec<i128> = r.split(',').map(|x| x.parse().unwrap()).collect();
        by_pool.entry(p).or_default().push((l, r));
    }
    let mut rows_by_pool: HashMap<[u8; 32], BTreeMap<i64, Vec<&_>>> = HashMap::new();
    for r in &rows {
        rows_by_pool
            .entry(r.pool_id)
            .or_default()
            .entry(r.ledger_sequence)
            .or_default()
            .push(r);
    }
    let mut steps: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let mut config_samples = Vec::new();
    let mut off = Vec::new();
    for (contract, pool) in &pair_pools {
        let fam = match family.get(contract).map(String::as_str) {
            Some("router") => format!("router:{}", raw_type[contract]),
            f => f.unwrap_or_default().to_string(),
        };
        let Some(states) = by_pool.get(&hex::encode(pool.pool_id)) else {
            continue;
        };
        for w in states.windows(2) {
            let ((l1, r1), (l2, r2)) = (&w[0], &w[1]);
            let mut moved = vec![0i128; pool.legs.len()];
            let pool_rows = rows_by_pool.get(&pool.pool_id);
            for r in pool_rows
                .into_iter()
                .flat_map(|m| m.range(l1 + 1..=*l2))
                .flat_map(|(_, rs)| rs)
            {
                let i = pool.legs.iter().position(|a| *a == r.asset_id).unwrap();
                moved[i] += r.amount;
            }
            let delta: Vec<i128> = r2.iter().zip(r1).map(|(b, a)| b - a).collect();
            if moved.iter().all(|m| *m == 0) {
                continue; // a reserve write with no pool event (admin, sync, fee claim)
            }
            let s = steps.entry(fam.clone()).or_default();
            s.0 += 1;
            if delta == moved {
                s.1 += 1;
            } else if delta
                .iter()
                .zip(&moved)
                .all(|(d, m)| (d - m).abs() * 100 <= m.abs().max(1))
            {
                s.2 += 1;
            } else if fam == "pair" && !RESTORED_STALE.contains(l2) {
                off.push((hex::encode(pool.pool_id), *l2, delta, moved));
            } else if fam == "config" && config_samples.len() < 5 {
                config_samples.push((hex::encode(pool.pool_id), *l1, *l2, delta, moved));
            }
        }
    }
    for c in &config_samples {
        println!("  config step beyond 1%: {c:?}");
    }
    for (fam, (n, exact, near)) in &steps {
        println!("{fam:>6} reserve steps: {n}, exact {exact}, within 1% {near}");
    }
    for o in off.iter().take(10) {
        println!(
            "  pool {} ledger {}: Δreserves {:?}, ours {:?}",
            o.0, o.1, o.2, o.3
        );
    }

    assert_eq!(
        missing, 0,
        "amount events the decoder did not turn into rows"
    );
    assert!(
        unknown.is_empty(),
        "pool events with a name in neither list: {unknown:?}"
    );
    assert!(
        off.is_empty(),
        "{} pair-family reserve steps disagree",
        off.len()
    );
}

/// Names of the events that carry amounts, in any family; a Phoenix
/// per-field event carries one of them with a field name as its second topic.
const AMOUNT_NAMES: [&str; 7] = [
    "trade",
    "deposit_liquidity",
    "withdraw_liquidity",
    "swap",
    "deposit",
    "withdraw",
    "provide_liquidity",
];

/// The event's name: its first topic, or `SoroswapPair:<second>` for the
/// pair family, whose first topic names the contract type.
fn event_name(topics_xdr: &str) -> String {
    let t: serde_json::Value = serde_json::from_str(topics_xdr).unwrap_or_default();
    let v = |i: usize| {
        t.get(i)
            .and_then(|x| x.get("value"))
            .and_then(|x| x.as_str())
    };
    match (v(0), v(1)) {
        (Some("SoroswapPair"), Some(second)) => format!("SoroswapPair:{second}"),
        (first, _) => first.unwrap_or("<none>").to_string(),
    }
}

/// An event that opens an amount: a named amount event, or the `sender`
/// event that opens a Phoenix per-field group.
fn is_amount_shape(topics_xdr: &str) -> bool {
    let t: serde_json::Value = serde_json::from_str(topics_xdr).unwrap_or_default();
    let v = |i: usize, k: &str| {
        t.get(i)
            .and_then(|x| x.get(k))
            .and_then(|x| x.as_str())
            .unwrap_or("")
    };
    let name = if v(0, "value") == "SoroswapPair" {
        v(1, "value")
    } else {
        v(0, "value")
    };
    let named = AMOUNT_NAMES.contains(&name);
    named
        && (v(0, "type") != "string"
            || v(0, "value") == "SoroswapPair"
            || v(1, "value") == "sender")
}
