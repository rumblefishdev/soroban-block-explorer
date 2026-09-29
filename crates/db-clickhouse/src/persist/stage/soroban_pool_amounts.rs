//! `soroban_pool_event_amounts`: what each swap, deposit and withdrawal event
//! of a registered soroban pool moved, one row per (event, leg), signed from
//! the pool's side — the soroban twin of `pool_operation_amounts`.
//!
//! Decoded from the staged [`SorobanEventRow`]s rather than from the parser's
//! events, so the live writer and the backfill (which reads `soroban_events`
//! back) run the very same function on the very same bytes.
//!
//! Only events emitted by a REGISTERED pool are read: the pair family names
//! its amounts by position (`amount_0` / `amount_1`), so the pool's `legs` are
//! needed to key them, and the registry is what proves the emitter is a pool.

use std::collections::HashMap;

use serde_json::Value;

use super::{StagedLedger, contract_token_asset_id};
use crate::persist::ids;
use crate::persist::rows::{SorobanEventRow, SorobanPoolEventAmountRow};

/// A registered soroban pool, keyed by its contract surrogate
/// (`soroban_events.contract_id`).
#[derive(Debug, Clone)]
pub struct SorobanPool {
    pub pool_id: [u8; 32],
    /// `liquidity_pools.legs`: asset ids in the pool's own token order.
    pub legs: Vec<i64>,
}

/// A registry entry keyed by the pool's contract surrogate, the key its
/// events carry.
pub fn soroban_pool_entry(pool_id: [u8; 32], legs: Vec<i64>) -> (i64, SorobanPool) {
    let strkey = stellar_strkey::Contract(pool_id).to_string();
    (ids::contract_id(&strkey), SorobanPool { pool_id, legs })
}

/// Fill [`StagedLedger::soroban_pool_amount_rows`] from the ledger's staged
/// events. A pool registered in this very ledger joins `pools` first, so its
/// first trades are not lost to the prefetch having run before it existed.
pub fn stage_soroban_pool_amounts(
    staged: &mut StagedLedger,
    pools: &HashMap<i64, SorobanPool>,
    sac_classic: &HashMap<i64, i64>,
) {
    let registered_now: Vec<(i64, SorobanPool)> = staged
        .pool_rows
        .iter()
        .filter(|r| r.pool_kind == 1)
        .map(|r| soroban_pool_entry(r.pool_id, r.legs.clone()))
        .collect();
    staged.soroban_pool_amount_rows = if registered_now.is_empty() {
        soroban_pool_amount_rows(&staged.event_rows, pools, sac_classic)
    } else {
        let mut all = pools.clone();
        all.extend(registered_now);
        soroban_pool_amount_rows(&staged.event_rows, &all, sac_classic)
    };
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
/// An event that is not a swap, deposit or withdrawal (`update_reserves`,
/// `sync`, `claim_fees`, …) is skipped. One that is but cannot be read is
/// logged at `error!` and skipped — never dropped silently.
pub fn soroban_pool_amount_rows(
    events: &[SorobanEventRow],
    pools: &HashMap<i64, SorobanPool>,
    sac_classic: &HashMap<i64, i64>,
) -> Vec<SorobanPoolEventAmountRow> {
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
                phoenix_legs(name, &fields, pool, sac_classic),
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
                phoenix_legs(name, &map_fields(&data), pool, sac_classic)
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
            _ => None,
        };
        push(&mut out, ev, pool, decoded);
    }
    out
}

fn unreadable(ev: &SorobanEventRow, why: &str) {
    tracing::error!(
        contract_id = ev.contract_id,
        ledger_sequence = ev.ledger_sequence,
        application_order = ev.application_order,
        event_index = ev.event_index,
        "soroban pool amount event skipped: {why}"
    );
}

/// Append one row per leg of an amount event, after checking every leg
/// belongs to the pool. An event whose legs cannot be read, or that names a
/// token the pool does not hold, is refused whole and logged.
fn push(
    out: &mut Vec<SorobanPoolEventAmountRow>,
    ev: &SorobanEventRow,
    pool: &SorobanPool,
    decoded: Option<Decoded>,
) {
    let Some((kind, legs)) = decoded else { return };
    let Some(legs) = legs else {
        unreadable(ev, "amount fields missing or malformed");
        return;
    };
    if legs.iter().any(|(a, _)| !pool.legs.contains(a)) {
        unreadable(ev, "names a token the pool does not hold");
        return;
    }
    // Every leg is written, a zero one too: a withdrawal paying out nothing
    // (183 in ledgers 62-63M) still burned shares, and would otherwise leave
    // no row at all.
    out.extend(
        legs.into_iter()
            .map(|(asset_id, amount)| SorobanPoolEventAmountRow {
                pool_id: pool.pool_id,
                ledger_sequence: ev.ledger_sequence,
                application_order: ev.application_order,
                operation_index: ev.operation_index,
                event_index: ev.event_index,
                event_kind: kind as u8,
                asset_id,
                amount,
            }),
    );
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
                pair_legs(
                    pool,
                    -amount("return_amount_a")?,
                    -amount("return_amount_b")?,
                )
            })(),
        ),
        _ => return None,
    })
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
