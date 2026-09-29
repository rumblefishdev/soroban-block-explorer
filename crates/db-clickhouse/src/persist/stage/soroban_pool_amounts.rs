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

/// Amount rows for every pool event in `events`. `events` may span many
/// contracts and transactions; per-field Phoenix events are grouped within
/// their operation in `event_index` order.
pub fn soroban_pool_amount_rows(
    events: &[SorobanEventRow],
    pools: &HashMap<i64, SorobanPool>,
    sac_classic: &HashMap<i64, i64>,
) -> Vec<SorobanPoolEventAmountRow> {
    let mut pool_events: Vec<&SorobanEventRow> = events
        .iter()
        .filter(|e| pools.contains_key(&e.contract_id))
        .collect();
    pool_events.sort_by_key(|e| {
        (
            e.contract_id,
            e.ledger_sequence,
            e.application_order,
            e.operation_index,
            e.event_index,
        )
    });

    let mut out = Vec::new();
    let mut i = 0;
    while i < pool_events.len() {
        let ev = pool_events[i];
        let pool = &pools[&ev.contract_id];
        let (Ok(topics), Ok(data)) = (
            serde_json::from_str::<Value>(&ev.topics_xdr),
            serde_json::from_str::<Value>(&ev.data_xdr),
        ) else {
            i += 1;
            continue;
        };
        let topics = topics.as_array().cloned().unwrap_or_default();
        let key = |row: &SorobanEventRow| {
            (
                row.contract_id,
                row.ledger_sequence,
                row.application_order,
                row.operation_index,
            )
        };

        // Phoenix, older format: one event per field, `("swap", "sender")`
        // opening each group. Gather the group's fields, then decode it once.
        if let Some(name) = phoenix_field_event_name(&topics)
            && str_value(&topics[1]) == Some("sender")
        {
            let mut fields: HashMap<String, Value> = HashMap::new();
            let mut j = i;
            while j < pool_events.len() && key(pool_events[j]) == key(ev) {
                let row = pool_events[j];
                let Ok(t) = serde_json::from_str::<Value>(&row.topics_xdr) else {
                    break;
                };
                let t = t.as_array().cloned().unwrap_or_default();
                if phoenix_field_event_name(&t) != Some(name)
                    || (j > i && str_value(&t[1]) == Some("sender"))
                {
                    break;
                }
                if let (Some(field), Ok(v)) = (
                    str_value(&t[1]),
                    serde_json::from_str::<Value>(&row.data_xdr),
                ) {
                    fields.insert(field.to_string(), v);
                }
                j += 1;
            }
            push(
                &mut out,
                ev,
                pool,
                phoenix_legs(name, &fields, pool, sac_classic),
            );
            i = j;
            continue;
        }

        let legs = match topic_names(&topics) {
            // Phoenix, newer format: `[swap]` etc. alone, with one map of the
            // same fields. Before the router arms: `withdraw_liquidity` is a
            // name both families use.
            (Some(name @ ("swap" | "provide_liquidity" | "withdraw_liquidity")), None)
                if topics.len() == 1 && type_of(&data) == Some("map") =>
            {
                phoenix_legs(name, &map_fields(&data), pool, sac_classic)
            }
            // Router family (Aquarius): tokens in the topics, amounts in a vec.
            (Some("trade"), _) => aquarius_trade(&topics, &data, sac_classic),
            (Some("deposit_liquidity"), _) => aquarius_liquidity(&topics, &data, 1, sac_classic),
            (Some("withdraw_liquidity"), _) => aquarius_liquidity(&topics, &data, -1, sac_classic),
            // Pair family (Soroswap): `["SoroswapPair", name]`, amounts by leg position.
            (Some("SoroswapPair"), Some("swap")) => soroswap_swap(&data, pool),
            (Some("SoroswapPair"), Some("deposit")) => soroswap_liquidity(&data, pool, 1),
            (Some("SoroswapPair"), Some("withdraw")) => soroswap_liquidity(&data, pool, -1),
            _ => None,
        };
        push(&mut out, ev, pool, legs);
        i += 1;
    }
    out
}

/// Append one row per leg, after checking every leg belongs to the pool. A
/// token the pool does not hold means the event is not what its shape claims,
/// so the whole event is refused with a warning rather than half-written.
fn push(
    out: &mut Vec<SorobanPoolEventAmountRow>,
    ev: &SorobanEventRow,
    pool: &SorobanPool,
    legs: Option<Vec<(i64, i128)>>,
) {
    let Some(legs) = legs else { return };
    if let Some((asset, _)) = legs.iter().find(|(a, _)| !pool.legs.contains(a)) {
        tracing::warn!(
            contract_id = ev.contract_id,
            ledger_sequence = ev.ledger_sequence,
            event_index = ev.event_index,
            asset_id = asset,
            "soroban pool event names a token the pool does not hold: event skipped"
        );
        return;
    }
    // A leg that nets to zero moved nothing (as in `pool_operation_amounts`).
    out.extend(
        legs.into_iter()
            .filter(|(_, amount)| *amount != 0)
            .map(|(asset_id, amount)| SorobanPoolEventAmountRow {
                pool_id: pool.pool_id,
                ledger_sequence: ev.ledger_sequence,
                application_order: ev.application_order,
                operation_index: ev.operation_index,
                event_index: ev.event_index,
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
fn phoenix_legs(
    name: &str,
    f: &HashMap<String, Value>,
    pool: &SorobanPool,
    sac: &HashMap<i64, i64>,
) -> Option<Vec<(i64, i128)>> {
    let token = |k: &str| Some(contract_token_asset_id(str_value(f.get(k)?)?, sac));
    let amount = |k: &str| int_value(f.get(k)?);
    match name {
        "swap" => Some(vec![
            (token("sell_token")?, amount("offer_amount")?),
            (token("buy_token")?, -amount("return_amount")?),
        ]),
        "provide_liquidity" => Some(vec![
            (token("token_a")?, amount("token_a-amount")?),
            (token("token_b")?, amount("token_b-amount")?),
        ]),
        "withdraw_liquidity" => pair_legs(
            pool,
            -amount("return_amount_a")?,
            -amount("return_amount_b")?,
        ),
        _ => None,
    }
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
        Value::Number(n) => n.as_i64().map(i128::from),
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
