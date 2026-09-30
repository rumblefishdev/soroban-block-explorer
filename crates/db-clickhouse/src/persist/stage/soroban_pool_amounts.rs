//! `pool_movements`: what each swap, deposit and withdrawal event
//! of a registered soroban pool moved, one row per (event, leg), signed from
//! the pool's side — the soroban twin of `pool_operation_amounts`.
//!
//! Decoded from the staged [`SorobanEventRow`]s rather than from the parser's
//! events, so the live writer and the backfill (which reads `soroban_events`
//! back) run the very same function on the very same bytes.
//!
//! A pool is recognised the way its state is — by the state rows its own
//! ledger-entry writes staged in the same ledger, never by a registry read, so
//! a ledger's movements depend on that ledger alone. Readers start from the
//! registry, as the reserve reader does, so a contract that only looks like a
//! pool is never read. Legs, needed where amounts are named by position, come
//! from the ledger too: a pair's own instance, a pool registered in it, and
//! for a Phoenix withdrawal the pool's own payouts in the same operation.

use std::collections::HashMap;

use serde_json::Value;

use super::{StagedLedger, contract_token_asset_id};
use crate::persist::ids;
use crate::persist::rows::{PoolMovementRow, SorobanEventRow};

/// A soroban pool, keyed by its contract surrogate
/// (`soroban_events.contract_id`).
#[derive(Debug, Clone, Default)]
pub struct SorobanPool {
    pub pool_id: [u8; 32],
    /// Asset ids in the pool's own token order (`liquidity_pools.legs`), where
    /// known; empty where nothing in the ledger shows them.
    pub legs: Vec<i64>,
    /// What the pool paid out in each operation of the ledger (asset id,
    /// amount), by `(application_order, operation_index)` — the legs of a
    /// Phoenix withdrawal. Empty when read back from `soroban_events`.
    pub payouts: HashMap<(i16, u16), Vec<(i64, i128)>>,
}

/// A registry entry keyed by the pool's contract surrogate, the key its
/// events carry.
pub fn soroban_pool_entry(pool_id: [u8; 32], legs: Vec<i64>) -> (i64, SorobanPool) {
    let strkey = stellar_strkey::Contract(pool_id).to_string();
    (
        ids::contract_id(&strkey),
        SorobanPool {
            pool_id,
            legs,
            ..SorobanPool::default()
        },
    )
}

/// The ledger's [`StagedLedger::pool_movement_rows`], from its staged events.
///
/// The pools are the ones this ledger staged state rows for: every amount event
/// of a registered pool has one in the same ledger (100% of 269,641 pool-ledgers
/// over 200k ledgers, production 2026-09-30). Their legs, where known: a pair's
/// own instance (`pair_legs`), a pool registered in this ledger; and each
/// pool's payouts, from which a Phoenix withdrawal reads its legs.
pub(super) fn pool_movement_rows(
    staged: &StagedLedger,
    pair_legs: &HashMap<[u8; 32], Vec<i64>>,
    sac_classic: &HashMap<i64, i64>,
) -> Vec<PoolMovementRow> {
    let mut pools: HashMap<i64, SorobanPool> = HashMap::new();
    for r in &staged.pool_state_change_rows {
        let legs = pair_legs.get(&r.pool_id).cloned().unwrap_or_default();
        let (contract, pool) = soroban_pool_entry(r.pool_id, legs);
        pools.entry(contract).or_insert(pool);
    }
    for r in staged.pool_rows.iter().filter(|r| r.pool_kind == 1) {
        let (contract, registered) = soroban_pool_entry(r.pool_id, r.legs.clone());
        if let Some(pool) = pools.get_mut(&contract)
            && pool.legs.is_empty()
        {
            pool.legs = registered.legs;
        }
    }
    for t in &staged.asset_transfer_rows {
        let (Some(from), Some(amount)) = (t.from_id, t.amount) else {
            continue;
        };
        if t.from_kind == "C"
            && let Some(pool) = pools.get_mut(&from)
        {
            let op = (t.application_order, t.op_index as u16);
            pool.payouts
                .entry(op)
                .or_default()
                .push((t.asset_id, amount));
        }
    }
    soroban_pool_amount_rows(&staged.event_rows, &pools, sac_classic)
}

/// Whether the ledger carries an event named like a pool amount event — the
/// gate for the SAC map, which keys their tokens. A name only: the emitter is
/// recognised as a pool in staging.
pub fn carries_pool_amounts(events: &[(String, Vec<xdr_parser::ExtractedEvent>)]) -> bool {
    events.iter().flat_map(|(_, evs)| evs).any(|ev| {
        matches!(
            ev.topics
                .get(0)
                .and_then(|t| t.get("value"))
                .and_then(Value::as_str),
            Some(
                "trade"
                    | "swap"
                    | "deposit_liquidity"
                    | "withdraw_liquidity"
                    | "provide_liquidity"
                    | "SoroswapPair"
            )
        )
    })
}

/// What an event did to the pool — stored, not inferred from the signs: a
/// trade may carry a zero leg (42 on production) and a withdrawal may pay out
/// nothing, which the signs alone would misread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PoolEventKind {
    Trade = 0,
    Deposit = 1,
    Withdrawal = 2,
}

/// An amount event, with its legs when they could be read.
type Decoded = (PoolEventKind, Option<Vec<(i64, i128)>>);

/// Amount rows for every pool event in `events`. `events` may span many
/// contracts and transactions; per-field Phoenix events are grouped within
/// their operation in `event_index` order.
///
/// An event known to carry no amount ([`NON_AMOUNT_EVENTS`]) is skipped. One
/// with a name in neither list is logged at `warn!` — a renamed `trade` would
/// otherwise zero a pool's volume without a trace — and one that is an amount
/// event but cannot be read at `error!`. Nothing is dropped silently.
pub fn soroban_pool_amount_rows(
    events: &[SorobanEventRow],
    pools: &HashMap<i64, SorobanPool>,
    sac_classic: &HashMap<i64, i64>,
) -> Vec<PoolMovementRow> {
    let mut pool_events: Vec<(&SorobanEventRow, &SorobanPool)> = events
        .iter()
        .filter_map(|e| Some((e, pools.get(&e.contract_id)?)))
        .collect();
    pool_events.sort_by_key(|(e, _)| {
        (
            e.contract_id,
            e.ledger_sequence,
            e.application_order,
            e.operation_index,
            e.event_index,
        )
    });
    let op = |e: &SorobanEventRow| {
        (
            e.contract_id,
            e.ledger_sequence,
            e.application_order,
            e.operation_index,
        )
    };

    let mut out = Vec::new();
    let mut i = 0;
    while i < pool_events.len() {
        let (ev, pool) = pool_events[i];
        i += 1;
        let (Ok(topics), Ok(data)) = (
            serde_json::from_str::<Value>(&ev.topics_xdr),
            serde_json::from_str::<Value>(&ev.data_xdr),
        ) else {
            unreadable(ev, "event JSON does not parse");
            continue;
        };
        let topics = topics.as_array().map(Vec::as_slice).unwrap_or_default();

        // Phoenix, older format: one event per field, `("swap", "sender")`
        // opening each group. Gather the group's fields, then decode it once.
        if let Some(name) = phoenix_field_event_name(topics) {
            if topics.get(1).and_then(str_value) != Some("sender") {
                unreadable(ev, "per-field pool event outside a `sender` group");
                continue;
            }
            let mut fields: HashMap<String, Value> = HashMap::new();
            while let Some(&(next, _)) = pool_events.get(i) {
                let t = serde_json::from_str::<Value>(&next.topics_xdr).ok();
                let t = t.as_ref().and_then(Value::as_array).map(Vec::as_slice);
                let field = t.and_then(|t| t.get(1)).and_then(str_value);
                if op(next) != op(ev)
                    || t.and_then(phoenix_field_event_name) != Some(name)
                    || field == Some("sender")
                {
                    break;
                }
                match (field, serde_json::from_str::<Value>(&next.data_xdr)) {
                    (Some(f), Ok(v)) => {
                        fields.insert(f.to_string(), v);
                    }
                    _ => unreadable(next, "per-field pool event does not parse"),
                }
                i += 1;
            }
            push(
                &mut out,
                ev,
                pool,
                phoenix_legs(name, &fields, pool, op_of(ev), sac_classic),
            );
            continue;
        }

        let decoded = match topic_names(topics) {
            // Phoenix, newer format: `[swap]` etc. alone, with one map of the
            // same fields. Before the router arms: `withdraw_liquidity` is a
            // name both families use.
            (Some(name @ ("swap" | "provide_liquidity" | "withdraw_liquidity")), None)
                if topics.len() == 1 && type_of(&data) == Some("map") =>
            {
                phoenix_legs(name, &map_fields(&data), pool, op_of(ev), sac_classic)
            }
            // Router family (Aquarius): tokens in the topics, amounts in a vec.
            (Some("trade"), _) => Some((
                PoolEventKind::Trade,
                aquarius_trade(topics, &data, sac_classic),
            )),
            (Some("deposit_liquidity"), _) => Some((
                PoolEventKind::Deposit,
                aquarius_liquidity(topics, &data, 1, sac_classic),
            )),
            (Some("withdraw_liquidity"), _) => Some((
                PoolEventKind::Withdrawal,
                aquarius_liquidity(topics, &data, -1, sac_classic),
            )),
            // Pair family (Soroswap): `["SoroswapPair", name]`, amounts by leg position.
            (Some("SoroswapPair"), Some("swap")) => {
                Some((PoolEventKind::Trade, soroswap_swap(&data, pool)))
            }
            (Some("SoroswapPair"), Some("deposit")) => {
                Some((PoolEventKind::Deposit, soroswap_liquidity(&data, pool, 1)))
            }
            (Some("SoroswapPair"), Some("withdraw")) => Some((
                PoolEventKind::Withdrawal,
                soroswap_liquidity(&data, pool, -1),
            )),
            (Some("SoroswapPair"), Some("sync" | "skim")) => None,
            (Some(name), _) if NON_AMOUNT_EVENTS.contains(&name) => None,
            (name, _) => {
                tracing::warn!(
                    contract_id = ev.contract_id,
                    ledger_sequence = ev.ledger_sequence,
                    event_index = ev.event_index,
                    name = name.unwrap_or("<none>"),
                    "soroban pool event with an unknown name skipped"
                );
                None
            }
        };
        push(&mut out, ev, pool, decoded);
    }
    out
}

/// Events a pool emits that carry no swap, deposit or withdrawal amount: every
/// other name registered pools have emitted over the full history (production,
/// 2026-09-29) — rewards, fees, admin, upgrades, state snapshots, and the
/// SEP-41 events of a pool that is its own share token.
pub const NON_AMOUNT_EVENTS: &[&str] = &[
    "update_reserves",
    "pool_state",
    "reserves_sync",
    "position_update",
    "claim_reward",
    "claim_fees",
    "claim_protocol_fee",
    "set_rewards_config",
    "set_rewards_state",
    "rewards_gauge_add",
    "rewards_gauge_remove",
    "rewards_gauge_claim",
    "rewards_gauge_schedule_reward",
    "set_privileged_addrs",
    "set_protocol_fee",
    "enable_emergency_mode",
    "disable_emergency_mode",
    "commit_transfer_ownership",
    "apply_transfer_ownership",
    "commit_upgrade",
    "apply_upgrade",
    "revert_upgrade",
    "executable_update",
    "initialize",
    "kill_deposit",
    "kill_swap",
    "kill_claim",
    "unkill_deposit",
    "unkill_swap",
    "unkill_claim",
    "toggle_trading",
    "blend_pool",
    "mint",
    "burn",
    "transfer",
];

fn unreadable(ev: &SorobanEventRow, why: &str) {
    tracing::error!(
        contract_id = ev.contract_id,
        ledger_sequence = ev.ledger_sequence,
        application_order = ev.application_order,
        event_index = ev.event_index,
        "soroban pool amount event skipped: {why}"
    );
}

/// Append one row per leg of an amount event. An event whose legs cannot be
/// read is refused whole and logged, and so is one naming a token the pool does
/// not hold — checked where the legs are known here (a pair, a pool registered
/// in this ledger, a looked-up pool); the reconciliation test checks the rest
/// against the registry.
fn push(
    out: &mut Vec<PoolMovementRow>,
    ev: &SorobanEventRow,
    pool: &SorobanPool,
    decoded: Option<Decoded>,
) {
    let Some((kind, legs)) = decoded else { return };
    let Some(legs) = legs else {
        unreadable(ev, "amount fields missing or malformed, or legs unknown");
        return;
    };
    if !pool.legs.is_empty() && legs.iter().any(|(a, _)| !pool.legs.contains(a)) {
        unreadable(ev, "names a token the pool does not hold");
        return;
    }
    // Every leg is written, a zero one too: a withdrawal paying out nothing
    // (183 in ledgers 62-63M) still burned shares, and would otherwise leave
    // no row at all.
    out.extend(legs.into_iter().map(|(asset_id, amount)| PoolMovementRow {
        pool_id: pool.pool_id,
        ledger_sequence: ev.ledger_sequence,
        application_order: ev.application_order,
        operation_index: ev.operation_index,
        event_index: ev.event_index,
        event_kind: kind as u8,
        asset_id,
        amount,
    }));
}

/// `[trade, token_in, token_out, caller]` / `[amount_in, amount_out, fee]`:
/// the trader's gross input enters, the output leaves.
fn aquarius_trade(
    topics: &[Value],
    data: &Value,
    sac: &HashMap<i64, i64>,
) -> Option<Vec<(i64, i128)>> {
    let amounts = vec_items(data)?;
    let token_in = contract_token_asset_id(str_value(topics.get(1)?)?, sac);
    let token_out = contract_token_asset_id(str_value(topics.get(2)?)?, sac);
    Some(vec![
        (token_in, int_value(amounts.first()?)?),
        (token_out, -int_value(amounts.get(1)?)?),
    ])
}

/// `[deposit_liquidity|withdraw_liquidity, token…]` / `[shares, amount…]`:
/// one amount per token named in the topics, after the share figure.
fn aquarius_liquidity(
    topics: &[Value],
    data: &Value,
    sign: i128,
    sac: &HashMap<i64, i64>,
) -> Option<Vec<(i64, i128)>> {
    let amounts = vec_items(data)?;
    let tokens = &topics[1..];
    if amounts.len() != tokens.len() + 1 {
        return None;
    }
    tokens
        .iter()
        .zip(&amounts[1..])
        .map(|(t, a)| {
            Some((
                contract_token_asset_id(str_value(t)?, sac),
                sign * int_value(a)?,
            ))
        })
        .collect()
}

/// `amount_{0,1}_{in,out}`: each leg nets its input against its output.
fn soroswap_swap(data: &Value, pool: &SorobanPool) -> Option<Vec<(i64, i128)>> {
    let f = map_fields(data);
    let net = |leg: usize| -> Option<i128> {
        Some(
            int_value(f.get(&format!("amount_{leg}_in"))?)?
                - int_value(f.get(&format!("amount_{leg}_out"))?)?,
        )
    };
    pair_legs(pool, net(0)?, net(1)?)
}

/// `amount_0` / `amount_1` of a deposit (+) or withdrawal (−).
fn soroswap_liquidity(data: &Value, pool: &SorobanPool, sign: i128) -> Option<Vec<(i64, i128)>> {
    let f = map_fields(data);
    pair_legs(
        pool,
        sign * int_value(f.get("amount_0")?)?,
        sign * int_value(f.get("amount_1")?)?,
    )
}

fn pair_legs(pool: &SorobanPool, a: i128, b: i128) -> Option<Vec<(i64, i128)>> {
    match pool.legs.as_slice() {
        [leg_a, leg_b] => Some(vec![(*leg_a, a), (*leg_b, b)]),
        _ => None,
    }
}

/// Phoenix fields, whichever format carried them. `token_a`/`token_b` are the
/// pool's legs in order (its `query_config`).
///
/// A swap is written as the trader sees it: `offer_amount` in (equal to the
/// `actual received amount` in 243,411 of 243,411 swaps), `return_amount`
/// out. The pool also pays a commission and a referral fee out of the output
/// side (vendor `do_swap`); those leave to other recipients and are not in
/// the row — as the router family's input is the trader's gross amount.
fn phoenix_legs(
    name: &str,
    f: &HashMap<String, Value>,
    pool: &SorobanPool,
    op: (i16, u16),
    sac: &HashMap<i64, i64>,
) -> Option<Decoded> {
    let token = |k: &str| Some(contract_token_asset_id(str_value(f.get(k)?)?, sac));
    let amount = |k: &str| int_value(f.get(k)?);
    // The map form names a deposit's amounts `actual_received_{a,b}`.
    let either = |k: &str, alt: &str| amount(k).or_else(|| amount(alt));
    Some(match name {
        "swap" => (
            PoolEventKind::Trade,
            (|| {
                Some(vec![
                    (token("sell_token")?, amount("offer_amount")?),
                    (token("buy_token")?, -amount("return_amount")?),
                ])
            })(),
        ),
        "provide_liquidity" => (
            PoolEventKind::Deposit,
            (|| {
                Some(vec![
                    (
                        token("token_a")?,
                        either("token_a-amount", "actual_received_a")?,
                    ),
                    (
                        token("token_b")?,
                        either("token_b-amount", "actual_received_b")?,
                    ),
                ])
            })(),
        ),
        "withdraw_liquidity" => (
            PoolEventKind::Withdrawal,
            (|| {
                let (a, b) = (amount("return_amount_a")?, amount("return_amount_b")?);
                if pool.legs.len() == 2 {
                    return pair_legs(pool, -a, -b);
                }
                // The event names its amounts by position and the pool writes
                // its token order (CONFIG) only at creation — but the pool pays
                // each amount out in this operation, so a leg is the asset of
                // the one payout of its amount (17 of 17 withdrawals in all
                // history, production 2026-09-30). Equal amounts cannot tell
                // the legs apart and are refused.
                let paid = pool.payouts.get(&op)?;
                let leg_of = |x: i128| -> Option<i64> {
                    let mut hits = paid.iter().filter(|(_, v)| *v == x);
                    let (asset, _) = hits.next()?;
                    hits.next().is_none().then_some(*asset)
                };
                (a != b).then_some(())?;
                Some(vec![(leg_of(a)?, -a), (leg_of(b)?, -b)])
            })(),
        ),
        _ => return None,
    })
}

/// The event's operation within its ledger, the key of a pool's payouts.
fn op_of(ev: &SorobanEventRow) -> (i16, u16) {
    (ev.application_order, ev.operation_index)
}

/// `("swap"|"provide_liquidity"|"withdraw_liquidity", field)` as two Strings:
/// the older Phoenix per-field convention.
fn phoenix_field_event_name(topics: &[Value]) -> Option<&'static str> {
    if topics.len() != 2 || type_of(&topics[1]) != Some("string") {
        return None;
    }
    match (type_of(&topics[0]), str_value(&topics[0])) {
        (Some("string"), Some("swap")) => Some("swap"),
        (Some("string"), Some("provide_liquidity")) => Some("provide_liquidity"),
        (Some("string"), Some("withdraw_liquidity")) => Some("withdraw_liquidity"),
        _ => None,
    }
}

/// First topic's value, and the second's when it is a Symbol.
fn topic_names(topics: &[Value]) -> (Option<&str>, Option<&str>) {
    let second = topics
        .get(1)
        .filter(|t| type_of(t) == Some("sym"))
        .and_then(str_value);
    (topics.first().and_then(str_value), second)
}

fn type_of(v: &Value) -> Option<&str> {
    v.get("type").and_then(Value::as_str)
}

fn str_value(v: &Value) -> Option<&str> {
    v.get("value").and_then(Value::as_str)
}

/// An integer ScVal (`i128`, `u128`, `i64`, …) — carried as a string or number.
fn int_value(v: &Value) -> Option<i128> {
    match v.get("value")? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.to_string().parse().ok(),
        _ => None,
    }
}

fn vec_items(data: &Value) -> Option<&Vec<Value>> {
    (type_of(data) == Some("vec")).then(|| data.get("value")?.as_array())?
}

fn map_fields(data: &Value) -> HashMap<String, Value> {
    data.get("value")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some((
                str_value(e.get("key")?)?.to_string(),
                e.get("value")?.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
