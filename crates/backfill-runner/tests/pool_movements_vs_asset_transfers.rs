//! MEASUREMENT (decision 152, not committed): can a pool movement's amounts
//! come from `asset_transfers` instead of a per-family parse of the pool
//! event's data?
//!
//! For every decoded movement (the W1 decoder over `soroban_events`), the
//! transfers that touch the pool contract in the same operation are
//! attributed to a pool amount event by position: a transfer at
//! `event_pos_in_op = p` belongs to the first of the pool's amount events
//! whose `event_index > p` (the token moves, then the pool reports). The
//! attributed net flow per leg (into the pool +, out −) is compared with the
//! decoded amount.
//!
//!   POOL_FROM=… POOL_TO=… cargo test -p backfill-runner \
//!     --test pool_movements_vs_asset_transfers -- --nocapture

use std::collections::{BTreeMap, HashMap, HashSet};

use db_clickhouse::persist::rows::SorobanEventRow;
use db_clickhouse::persist::stage;

#[derive(clickhouse::Row, serde::Deserialize, Debug)]
struct TransferRow {
    ledger_sequence: i64,
    application_order: i16,
    op_index: i16,
    event_pos_in_op: i16,
    asset_id: i64,
    amount: String,
    from_pool: i64,
    to_pool: i64,
}

#[tokio::test(flavor = "multi_thread")]
async fn movements_match_attributed_transfers() {
    let user = std::env::var("USER").unwrap_or_default();
    let local = format!(
        "{}/../../infra-hetzner/ca/out/{user}/{user}",
        env!("CARGO_MANIFEST_DIR")
    );
    let (cert, key) = (format!("{local}.crt"), format!("{local}.key"));
    if !std::path::Path::new(&cert).exists() {
        eprintln!("no client certificate — skipping");
        return;
    }
    let bundle = db_clickhouse::mtls::MtlsBundle {
        cert_pem: std::fs::read_to_string(&cert).unwrap(),
        key_pem: std::fs::read_to_string(&key).unwrap(),
        ca_pem: String::new(),
    };
    let ch = db_clickhouse::mtls::client_with_mtls(
        "ch.sorobanscan.rumblefish.dev",
        &bundle,
        db_clickhouse::PROD_DATABASE,
    )
    .unwrap();

    let pools = db_clickhouse::persist::fetch_soroban_pools(&ch, true)
        .await
        .unwrap();
    let sac = db_clickhouse::persist::fetch_sac_classic_map(&ch, true)
        .await
        .unwrap();
    let family: HashMap<[u8; 32], String> = ch
        .query(
            "SELECT lower(hex(pool_id)), any(pool_type_raw) FROM liquidity_pools \
             WHERE pool_kind = 1 GROUP BY pool_id",
        )
        .fetch_all::<(String, String)>()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(h, raw)| {
            let id: [u8; 32] = hex::decode(h).ok()?.try_into().ok()?;
            let f = match raw.as_str() {
                "" => "pair".to_string(),
                r if r.bytes().all(|b| b.is_ascii_digit()) => "config".to_string(),
                r => format!("router:{r}"),
            };
            Some((id, f))
        })
        .collect();
    let contract_of: HashMap<[u8; 32], i64> = pools.iter().map(|(c, p)| (p.pool_id, *c)).collect();

    let tip: i64 = ch
        .query("SELECT max(ledger_sequence) FROM soroban_events")
        .fetch_one()
        .await
        .unwrap();
    let env = |k: &str| std::env::var(k).ok().map(|v| v.parse::<i64>().unwrap());
    let to = env("POOL_TO").unwrap_or(tip).min(tip);
    let from = env("POOL_FROM").unwrap_or(to - 200_000);
    let ids = pools
        .keys()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");

    let events: Vec<SorobanEventRow> = ch
        .query(&format!(
            "SELECT contract_id, ledger_sequence, transaction_index, operation_index, \
                    event_index, application_order, event_type, signature, topics_xdr, data_xdr \
             FROM soroban_events \
             WHERE contract_id IN ({ids}) AND ledger_sequence > {from} AND ledger_sequence <= {to} \
             LIMIT 1 BY contract_id, ledger_sequence, transaction_index, operation_index, event_index"
        ))
        .fetch_all()
        .await
        .unwrap();
    let rows = stage::soroban_pool_amounts::soroban_pool_amount_rows(&events, &pools, &sac);

    let transfers: Vec<TransferRow> = ch
        .query(&format!(
            "SELECT ledger_sequence, application_order, op_index, event_pos_in_op, asset_id, \
                    toString(ifNull(amount, 0)) AS amount, \
                    if(from_kind = 'C', ifNull(from_id, 0), 0) AS from_pool, \
                    if(to_kind = 'C', ifNull(to_id, 0), 0) AS to_pool \
             FROM asset_transfers \
             WHERE ledger_sequence > {from} AND ledger_sequence <= {to} \
               AND ((from_kind = 'C' AND from_id IN ({ids})) OR (to_kind = 'C' AND to_id IN ({ids}))) \
             LIMIT 1 BY ledger_sequence, application_order, op_index, event_pos_in_op"
        ))
        .fetch_all()
        .await
        .unwrap();

    // Movement events per (pool contract, ledger, app order, op), by position.
    type Op = (i64, i64, i16, u16);
    let mut decoded: BTreeMap<(Op, u32), BTreeMap<i64, i128>> = BTreeMap::new();
    let mut kinds: HashMap<(Op, u32), u8> = HashMap::new();
    for r in &rows {
        let op = (
            contract_of[&r.pool_id],
            r.ledger_sequence,
            r.application_order,
            r.operation_index,
        );
        *decoded
            .entry((op, r.event_index))
            .or_default()
            .entry(r.asset_id)
            .or_default() += r.amount;
        kinds.insert((op, r.event_index), r.event_kind);
    }
    let mut event_positions: HashMap<Op, Vec<u32>> = HashMap::new();
    for (op, e) in decoded.keys() {
        event_positions.entry(*op).or_default().push(*e);
    }

    // Attribute each pool transfer to the pool's next amount event in its op.
    let mut attributed: HashMap<(Op, u32), BTreeMap<i64, i128>> = HashMap::new();
    let mut after_last = 0u64;
    let mut no_event_op = 0u64;
    for t in &transfers {
        let amount: i128 = t.amount.parse().unwrap();
        for (pool, sign) in [(t.to_pool, 1i128), (t.from_pool, -1i128)] {
            if pool == 0 || !pools.contains_key(&pool) {
                continue;
            }
            let op = (
                pool,
                t.ledger_sequence,
                t.application_order,
                t.op_index as u16,
            );
            let Some(positions) = event_positions.get(&op) else {
                no_event_op += 1;
                continue;
            };
            match positions
                .iter()
                .find(|e| **e as i64 > t.event_pos_in_op as i64)
            {
                Some(e) => {
                    *attributed
                        .entry((op, *e))
                        .or_default()
                        .entry(t.asset_id)
                        .or_default() += sign * amount;
                }
                None => after_last += 1,
            }
        }
    }

    // Compare per movement, on the pool's legs only (a share-token mint/burn
    // or a reward token that is not a leg is not a movement amount).
    let mut stats: BTreeMap<(String, u8), (u64, u64)> = BTreeMap::new();
    let mut samples: BTreeMap<(String, u8), Vec<String>> = BTreeMap::new();
    for ((op, e), legs) in &decoded {
        let pool = &pools[&op.0];
        let fam = family.get(&pool.pool_id).cloned().unwrap_or_default();
        let kind = kinds[&(*op, *e)];
        let got = attributed.get(&(*op, *e));
        let leg_set: HashSet<i64> = pool.legs.iter().copied().collect();
        let equal = legs
            .iter()
            .all(|(a, amt)| got.and_then(|g| g.get(a)).copied().unwrap_or(0) == *amt)
            && got.is_none_or(|g| {
                g.iter()
                    .filter(|(a, _)| leg_set.contains(a))
                    .all(|(a, v)| legs.get(a).copied().unwrap_or(0) == *v)
            });
        let s = stats.entry((fam.clone(), kind)).or_default();
        s.0 += 1;
        if equal {
            s.1 += 1;
        } else {
            let v = samples.entry((fam, kind)).or_default();
            if v.len() < 3 {
                v.push(format!("{op:?} ev {e}: decoded {legs:?} transfers {got:?}"));
            }
        }
    }
    println!(
        "window {from}..={to}: {} events, {} movements, {} pool transfers; \
         transfers after the op's last pool event {after_last}, in ops with no movement {no_event_op}",
        events.len(),
        decoded.len(),
        transfers.len()
    );
    for ((fam, kind), (n, eq)) in &stats {
        println!(
            "{fam:>22} kind {kind}: {n:>8} movements, equal {eq:>8} ({:.3}%)",
            100.0 * *eq as f64 / *n as f64
        );
    }
    for ((fam, kind), v) in &samples {
        for s in v {
            println!("  {fam} {kind}: {s}");
        }
    }
}
