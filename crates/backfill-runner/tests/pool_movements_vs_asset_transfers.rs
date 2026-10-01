//! Task 0374 (W1) — an independent check of the pool movement decoder:
//! the token transfers (`asset_transfers`) that touch a pool against the
//! amounts decoded from the pool's own events, read-only, on production.
//!
//! Checked per OPERATION where the pool emitted exactly one amount event and
//! no payout — there every transfer touching the pool is that movement's
//! (which transfer belongs to which event otherwise depends on each family's
//! own ordering). Net flow per leg, into the pool +, out −, must equal the
//! decoded amount, except two known classes:
//! - a leg token that emits no standard SEP-41 `transfer` (one emits its own
//!   `transfer_event`), which `asset_transfers` rightly skips — only the pool
//!   event carries that amount;
//! - a config-family swap, whose pool pays a commission to a third party out
//!   of the output side: the event reports what the trader got;
//! - a surplus: more moved through the pool's balance than its reserves
//!   count (an exhausted swap keeps its whole input; a router sends a token
//!   beyond the traded amount) — the reserves side with the event;
//! - a burn out of the pool's balance in the same operation (a token that
//!   burns on transfer, or the pool burning a leg it holds): the transfers
//!   minus the burns equal the event;
//! - a pair deposit whose tokens reached the pool in an earlier transaction:
//!   no leg moves in the operation, and the reserves side with the event;
//! - a four-token pool whose event names only three tokens (a topic holds at
//!   most four values): the named legs agree, the fourth has no amount;
//! - the ledgers where a pair pool was restored from a stale copy, listed in
//!   the reconciliation test too.
//!
//! Measured 2026-09-30 over the whole history (50.6M to the tip, 1M-ledger
//! windows): every compared movement equal or in a known class. ~0.5% of
//! movements are not compared (several amount events or a payout in one
//! operation). Skips without a client certificate.
//!
//!   POOL_FROM=… POOL_TO=… cargo test -p backfill-runner \
//!     --test pool_movements_vs_asset_transfers -- --nocapture

use std::collections::{BTreeMap, HashMap, HashSet};

use db_clickhouse::persist::rows::SorobanEventRow;
use db_clickhouse::persist::stage;

/// An operation of one pool: (pool contract, ledger, application order, op).
type Op = (i64, i64, i16, u16);
/// Decoded movements by (operation, event index): amount per leg asset.
type Movements = BTreeMap<(Op, u32), BTreeMap<i64, i128>>;

/// Ledgers where a pair pool's instance was restored from a stale copy after
/// protocol 23: its reserves jump back with no pool event (the same list as
/// the reconciliation test's).
const RESTORED_STALE: [i64; 3] = [58_774_376, 58_779_504, 58_779_518];

#[derive(clickhouse::Row, serde::Deserialize, Debug)]
struct TransferRow {
    ledger_sequence: i64,
    application_order: i16,
    op_index: i16,
    asset_id: i64,
    amount: String,
    from_pool: i64,
    to_pool: i64,
    burn: bool,
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

    let pools = db_clickhouse::persist::fetch_soroban_pools(&ch)
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

    // Both reads go in 50k-ledger slices to stay under the read profile's
    // 30 s cap on a busy server.
    let slices: Vec<(i64, i64)> = (from..to)
        .step_by(50_000)
        .map(|lo| (lo, (lo + 50_000).min(to)))
        .collect();
    let mut events: Vec<SorobanEventRow> = Vec::new();
    for (lo, hi) in &slices {
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

    let mut transfers: Vec<TransferRow> = Vec::new();
    for (lo, hi) in &slices {
        transfers.extend(
            ch.query(&format!(
                "SELECT ledger_sequence, application_order, op_index, asset_id, \
                        toString(ifNull(amount, 0)) AS amount, \
                        if(from_kind = 'C', ifNull(from_id, 0), 0) AS from_pool, \
                        if(to_kind = 'C', ifNull(to_id, 0), 0) AS to_pool, \
                        verb = 'burn' AS burn \
                 FROM asset_transfers \
                 WHERE ledger_sequence > {lo} AND ledger_sequence <= {hi} \
                   AND ((from_kind = 'C' AND from_id IN ({ids})) OR (to_kind = 'C' AND to_id IN ({ids}))) \
                 LIMIT 1 BY ledger_sequence, application_order, op_index, event_pos_in_op"
            ))
            .fetch_all::<TransferRow>()
            .await
            .unwrap(),
        );
    }

    // Movements per (pool contract, ledger, app order, op).
    let mut decoded: Movements = BTreeMap::new();
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

    // Which transfer belongs to which event depends on each family's own
    // order of transfers and events (a concentrated withdrawal reports
    // `claim_fees` between its transfers and its event), so the check is per
    // OPERATION: compared are the ops where the pool emitted exactly one
    // amount event and no payout (`claim_fees`, `claim_reward`, …) — there,
    // every transfer touching the pool is that movement's.
    const PAYOUTS: [&str; 6] = [
        "claim_fees",
        "claim_reward",
        "claim_protocol_fee",
        "rewards_gauge_claim",
        "SoroswapPair:skim",
        "reserves_sync",
    ];
    let mut amount_events: HashMap<Op, Vec<u32>> = HashMap::new();
    for (op, e) in decoded.keys() {
        amount_events.entry(*op).or_default().push(*e);
    }
    let mut has_payout: HashSet<Op> = HashSet::new();
    for ev in &events {
        let t: serde_json::Value = serde_json::from_str(&ev.topics_xdr).unwrap_or_default();
        let v = |i: usize| {
            t.get(i)
                .and_then(|x| x.get("value"))
                .and_then(|x| x.as_str())
        };
        let name = match (v(0), v(1)) {
            (Some("SoroswapPair"), Some(second)) => format!("SoroswapPair:{second}"),
            (first, _) => first.unwrap_or_default().to_string(),
        };
        if PAYOUTS.contains(&name.as_str()) {
            has_payout.insert((
                ev.contract_id,
                ev.ledger_sequence,
                ev.application_order,
                ev.operation_index,
            ));
        }
    }
    let mut attributed: HashMap<(Op, u32), BTreeMap<i64, i128>> = HashMap::new();
    // The same flows without the burns out of the pool's balance.
    let mut unburnt: HashMap<(Op, u32), BTreeMap<i64, i128>> = HashMap::new();
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
            if let Some([e]) = amount_events.get(&op).map(Vec::as_slice) {
                *attributed
                    .entry((op, *e))
                    .or_default()
                    .entry(t.asset_id)
                    .or_default() += sign * amount;
                if !t.burn {
                    *unburnt
                        .entry((op, *e))
                        .or_default()
                        .entry(t.asset_id)
                        .or_default() += sign * amount;
                }
            }
        }
    }
    let compared =
        |op: &Op| amount_events.get(op).is_some_and(|v| v.len() == 1) && !has_payout.contains(op);

    // Tokens with no standard transfer in the window: a token that emits its
    // own event (`transfer_event`) instead of SEP-41 `transfer` never reaches
    // `asset_transfers`, so only the pool event carries its amount.
    let seen: HashSet<i64> = transfers.iter().map(|t| t.asset_id).collect();

    // Compare per movement, on the pool's legs only (a share-token mint/burn
    // or a reward token that is not a leg is not a movement amount).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Verdict {
        Equal,
        /// A leg's token emits no standard transfers; the other legs agree.
        NonStandardToken,
        /// Config family swap: the pool pays a commission to a third party out
        /// of the output side, so more leaves the pool than the trader gets.
        Commission,
        /// The pool's balance holds tokens its reserves do not count, so the
        /// transfers and the event differ in size, never in direction: a swap
        /// that exhausted the pool keeps its whole input but prices only what
        /// it used; a router sends a token beyond the amount traded; a pair
        /// counts dust already in its balance as input. The judge is the
        /// change of the pool's own reserves: on every leg where the two
        /// differ, the event must be the closer of the two.
        Surplus,
        /// Burns out of the pool's balance in the same operation; without
        /// them the transfers equal the event.
        Burn,
        /// A pair deposit paid in an earlier transaction: no leg moves in the
        /// operation, and the pool's reserves side with the event.
        PaidEarlier,
        /// A four-token pool's event names three tokens; those agree.
        LegNotInEvent,
        /// A pair pool restored from a stale copy in this ledger.
        RestoredStale,
        Other,
    }
    let mut stats: BTreeMap<(String, u8), BTreeMap<Verdict, u64>> = BTreeMap::new();
    let mut other: Vec<String> = Vec::new();
    let mut not_compared = 0u64;
    for ((op, e), legs) in &decoded {
        if !compared(op) {
            not_compared += 1;
            continue;
        }
        let pool = &pools[&op.0];
        let fam = family.get(&pool.pool_id).cloned().unwrap_or_default();
        let kind = kinds[&(*op, *e)];
        let flow = |a: &i64| {
            attributed
                .get(&(*op, *e))
                .and_then(|g| g.get(a))
                .copied()
                .unwrap_or(0)
        };
        let extra_leg = attributed.get(&(*op, *e)).is_some_and(|g| {
            g.iter()
                .any(|(a, v)| pool.legs.contains(a) && !legs.contains_key(a) && *v != 0)
        });
        let equal_on = |pred: &dyn Fn(&i64, &i128) -> bool| {
            !extra_leg
                && legs
                    .iter()
                    .filter(|(a, m)| pred(a, m))
                    .all(|(a, m)| flow(a) == *m)
        };
        let unburnt_flow = |a: &i64| {
            unburnt
                .get(&(*op, *e))
                .and_then(|g| g.get(a))
                .copied()
                .unwrap_or(0)
        };
        let verdict = if equal_on(&|_, _| true) {
            Verdict::Equal
        } else if RESTORED_STALE.contains(&op.1) {
            Verdict::RestoredStale
        } else if !extra_leg
            && unburnt.get(&(*op, *e)) != attributed.get(&(*op, *e))
            && legs.iter().all(|(a, m)| unburnt_flow(a) == *m)
        {
            Verdict::Burn
        } else if fam == "pair"
            && kind == 1
            && legs.keys().all(|a| flow(a) == 0)
            && reserves_side_with_event(&ch, &decoded, pool, *op, legs, &flow).await
        {
            Verdict::PaidEarlier
        } else if pool.legs.len() == 4 && legs.len() == 3 && legs.iter().all(|(a, m)| flow(a) == *m)
        {
            Verdict::LegNotInEvent
        } else if legs.keys().any(|a| !seen.contains(a)) && equal_on(&|a, _| seen.contains(a)) {
            Verdict::NonStandardToken
        } else if fam == "config"
            && kind == 0
            && equal_on(&|_, m| *m > 0)
            && legs.iter().all(|(a, m)| *m > 0 || flow(a) <= *m)
        {
            Verdict::Commission
        } else if !extra_leg
            && legs.iter().all(|(a, m)| {
                let f = flow(a);
                f == *m || f.signum() == m.signum()
            })
            && reserves_side_with_event(&ch, &decoded, pool, *op, legs, &flow).await
        {
            Verdict::Surplus
        } else {
            Verdict::Other
        };
        *stats
            .entry((fam.clone(), kind))
            .or_default()
            .entry(verdict)
            .or_default() += 1;
        if verdict == Verdict::Other {
            other.push(format!(
                "{fam} kind {kind} {op:?} ev {e}: decoded {legs:?} transfers {:?}",
                attributed.get(&(*op, *e))
            ));
        }
    }
    println!(
        "window {from}..={to}: {} events, {} movements, {} pool transfers; \
         not compared (several amount events or a payout in the op): {not_compared}",
        events.len(),
        decoded.len(),
        transfers.len()
    );
    for ((fam, kind), v) in &stats {
        println!("{fam:>22} kind {kind}: {v:?}");
    }
    for o in other.iter().take(20) {
        println!("  other: {o}");
    }
    assert!(
        other.is_empty(),
        "{} movements disagree with their transfers outside the known classes",
        other.len()
    );
}

/// Whether the pool's own reserves side with its event against the transfers:
/// between the state row this movement wrote and the one before it — and only
/// when no other movement of the pool falls in between — each leg where the
/// two differ must have moved closer to the event's amount than to the
/// transfers'.
async fn reserves_side_with_event(
    ch: &clickhouse::Client,
    decoded: &Movements,
    pool: &stage::soroban_pool_amounts::SorobanPool,
    op: Op,
    legs: &BTreeMap<i64, i128>,
    flow: &dyn Fn(&i64) -> i128,
) -> bool {
    let ledger = op.1;
    let states: Vec<(i64, String)> = ch
        .query(&format!(
            "SELECT ledger_sequence, arrayStringConcat(arrayMap(x -> toString(x), any(reserves)), ',') \
             FROM pool_state_changes \
             WHERE pool_id = unhex('{}') AND ledger_sequence <= {ledger} \
             GROUP BY ledger_sequence ORDER BY ledger_sequence DESC LIMIT 2",
            hex::encode(pool.pool_id)
        ))
        .fetch_all()
        .await
        .unwrap();
    // A pool's first state row starts from empty reserves.
    let empty = (0i64, vec!["0"; pool.legs.len()].join(","));
    let (after_at, after, before_at, before) = match states.as_slice() {
        [(a_at, a), (b_at, b)] => (*a_at, a, *b_at, b),
        [(a_at, a)] => (*a_at, a, empty.0, &empty.1),
        _ => return false,
    };
    if after_at != ledger {
        return false;
    }
    let alone = decoded.keys().all(|((c, l, a, o), _)| {
        *c != op.0 || *l <= before_at || *l > ledger || (*l, *a, *o) == (op.1, op.2, op.3)
    });
    if !alone {
        return false;
    }
    let parse = |r: &String| -> Vec<i128> { r.split(',').map(|x| x.parse().unwrap()).collect() };
    let (after, before) = (parse(after), parse(before));
    legs.iter().all(|(a, m)| {
        let f = flow(a);
        if f == *m {
            return true;
        }
        let Some(i) = pool.legs.iter().position(|l| l == a) else {
            return false;
        };
        let delta = after[i] - before[i];
        (delta - m).abs() < (delta - f).abs()
    })
}
