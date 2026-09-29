use std::collections::HashMap;

use super::*;
use crate::persist::ids;

const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
const PYUSD: &str = "CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2";
const ROUTER: &str = "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK";
const POOL: i64 = 7;
/// The classic USDC asset the SAC wraps.
const USDC: i64 = 42;

fn sym(s: &str) -> String {
    format!(r#"{{"type":"sym","value":"{s}"}}"#)
}
fn string(s: &str) -> String {
    format!(r#"{{"type":"string","value":"{s}"}}"#)
}
fn addr(s: &str) -> String {
    format!(r#"{{"type":"address","value":"{s}"}}"#)
}
fn i128v(n: i128) -> String {
    format!(r#"{{"type":"i128","value":"{n}"}}"#)
}
fn vec_of(items: &[String]) -> String {
    format!(r#"{{"type":"vec","value":[{}]}}"#, items.join(","))
}
fn map_of(entries: &[(&str, String)]) -> String {
    let e: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!(r#"{{"key":{},"value":{v}}}"#, sym(k)))
        .collect();
    format!(r#"{{"type":"map","value":[{}]}}"#, e.join(","))
}

fn event(event_index: u32, topics: &[String], data: String) -> SorobanEventRow {
    SorobanEventRow {
        contract_id: POOL,
        ledger_sequence: 100,
        transaction_index: 3,
        operation_index: 0,
        event_index,
        application_order: 3,
        event_type: 1,
        signature: None,
        topics_xdr: format!("[{}]", topics.join(",")),
        data_xdr: data,
    }
}

fn pools() -> HashMap<i64, SorobanPool> {
    HashMap::from([(
        POOL,
        SorobanPool {
            pool_id: [9; 32],
            legs: vec![ids::contract_id(PYUSD), USDC],
        },
    )])
}

fn sac() -> HashMap<i64, i64> {
    HashMap::from([(ids::contract_id(USDC_SAC), USDC)])
}

fn legs(rows: &[SorobanPoolEventAmountRow]) -> Vec<(u32, i64, i128)> {
    rows.iter()
        .map(|r| (r.event_index, r.asset_id, r.amount))
        .collect()
}

#[test]
fn aquarius_trade_enters_gross_and_leaves_the_output() {
    let ev = event(
        0,
        &[sym("trade"), addr(PYUSD), addr(USDC_SAC), addr(ROUTER)],
        vec_of(&[i128v(5_955_800), i128v(5_949_501), i128v(5_956)]),
    );
    let rows = soroban_pool_amount_rows(&[ev], &pools(), &sac());
    assert_eq!(
        legs(&rows),
        vec![
            (0, ids::contract_id(PYUSD), 5_955_800),
            (0, USDC, -5_949_501)
        ]
    );
    assert_eq!(rows[0].pool_id, [9; 32]);
    assert_eq!((rows[0].application_order, rows[0].operation_index), (3, 0));
}

#[test]
fn aquarius_liquidity_skips_the_share_figure() {
    let deposit = event(
        0,
        &[sym("deposit_liquidity"), addr(PYUSD), addr(USDC_SAC)],
        vec_of(&[i128v(3_841), i128v(1_930), i128v(1_916)]),
    );
    let withdraw = event(
        1,
        &[sym("withdraw_liquidity"), addr(PYUSD), addr(USDC_SAC)],
        vec_of(&[i128v(3_841), i128v(1_929), i128v(1_915)]),
    );
    let rows = soroban_pool_amount_rows(&[deposit, withdraw], &pools(), &sac());
    let pyusd = ids::contract_id(PYUSD);
    assert_eq!(
        legs(&rows),
        vec![
            (0, pyusd, 1_930),
            (0, USDC, 1_916),
            (1, pyusd, -1_929),
            (1, USDC, -1_915)
        ]
    );
}

#[test]
fn soroswap_amounts_key_by_leg_position() {
    let swap = event(
        0,
        &[string("SoroswapPair"), sym("swap")],
        map_of(&[
            ("amount_0_in", i128v(2_653)),
            ("amount_0_out", i128v(0)),
            ("amount_1_in", i128v(0)),
            ("amount_1_out", i128v(534)),
            ("to", addr(ROUTER)),
        ]),
    );
    let withdraw = event(
        1,
        &[string("SoroswapPair"), sym("withdraw")],
        map_of(&[
            ("amount_0", i128v(60)),
            ("amount_1", i128v(9)),
            ("liquidity", i128v(23)),
        ]),
    );
    let rows = soroban_pool_amount_rows(&[swap, withdraw], &pools(), &sac());
    let pyusd = ids::contract_id(PYUSD);
    assert_eq!(
        legs(&rows),
        vec![
            (0, pyusd, 2_653),
            (0, USDC, -534),
            (1, pyusd, -60),
            (1, USDC, -9)
        ]
    );
}

/// The older Phoenix convention publishes one event per field; the group
/// opened by `sender` decodes into one swap, keyed by its first event.
#[test]
fn phoenix_per_field_swap_groups_into_one_event() {
    let field =
        |i: u32, name: &str, value: String| event(i, &[string("swap"), string(name)], value);
    let evs = vec![
        field(4, "sender", addr(ROUTER)),
        field(5, "sell_token", addr(USDC_SAC)),
        field(6, "offer_amount", i128v(1_000)),
        field(7, "buy_token", addr(PYUSD)),
        field(8, "return_amount", i128v(990)),
        field(9, "spread_amount", i128v(1)),
        // A second swap in the same operation opens a new group.
        field(10, "sender", addr(ROUTER)),
        field(11, "sell_token", addr(PYUSD)),
        field(12, "offer_amount", i128v(50)),
        field(13, "buy_token", addr(USDC_SAC)),
        field(14, "return_amount", i128v(49)),
    ];
    let rows = soroban_pool_amount_rows(&evs, &pools(), &sac());
    let pyusd = ids::contract_id(PYUSD);
    assert_eq!(
        legs(&rows),
        vec![
            (4, USDC, 1_000),
            (4, pyusd, -990),
            (10, pyusd, 50),
            (10, USDC, -49)
        ]
    );
}

#[test]
fn phoenix_liquidity_in_either_format() {
    let field = |i: u32, name: &str, value: String| {
        event(i, &[string("provide_liquidity"), string(name)], value)
    };
    let mut evs = vec![
        field(0, "sender", addr(ROUTER)),
        field(1, "token_a", addr(PYUSD)),
        field(2, "token_a-amount", i128v(70)),
        field(3, "token_b", addr(USDC_SAC)),
        field(4, "token_b-amount", i128v(80)),
    ];
    // Newer format: `[withdraw_liquidity]` alone, one map of the same fields.
    evs.push(event(
        5,
        &[sym("withdraw_liquidity")],
        map_of(&[
            ("sender", addr(ROUTER)),
            ("shares_amount", i128v(10)),
            ("return_amount_a", i128v(7)),
            ("return_amount_b", i128v(8)),
        ]),
    ));
    let rows = soroban_pool_amount_rows(&evs, &pools(), &sac());
    let pyusd = ids::contract_id(PYUSD);
    assert_eq!(
        legs(&rows),
        vec![(0, pyusd, 70), (0, USDC, 80), (5, pyusd, -7), (5, USDC, -8)]
    );
}

#[test]
fn only_registered_pools_and_their_own_tokens_are_read() {
    let mut stranger = event(
        0,
        &[sym("trade"), addr(PYUSD), addr(USDC_SAC), addr(ROUTER)],
        vec_of(&[i128v(1), i128v(1), i128v(0)]),
    );
    stranger.contract_id = 8;
    // A trade naming a token the pool does not hold is refused whole.
    let foreign = event(
        1,
        &[sym("trade"), addr(PYUSD), addr(ROUTER), addr(ROUTER)],
        vec_of(&[i128v(1), i128v(1), i128v(0)]),
    );
    assert!(soroban_pool_amount_rows(&[stranger, foreign], &pools(), &sac()).is_empty());
}

/// 42 trades on production have a zero `amount_in` or `amount_out`: both legs
/// are written, and the stored kind says trade whatever the signs show.
#[test]
fn a_trade_with_a_zero_leg_stays_a_trade() {
    let ev = event(
        0,
        &[sym("trade"), addr(PYUSD), addr(USDC_SAC), addr(ROUTER)],
        vec_of(&[i128v(500), i128v(0), i128v(0)]),
    );
    let rows = soroban_pool_amount_rows(&[ev], &pools(), &sac());
    assert_eq!(
        legs(&rows),
        vec![(0, ids::contract_id(PYUSD), 500), (0, USDC, 0)]
    );
    assert!(
        rows.iter()
            .all(|r| r.event_kind == PoolEventKind::Trade as u8)
    );
}

/// The map-form Phoenix deposit names its amounts `actual_received_{a,b}`
/// (18 events on production were dropped before this was read).
#[test]
fn phoenix_map_deposit_reads_actual_received() {
    let ev = event(
        0,
        &[sym("provide_liquidity")],
        map_of(&[
            ("actual_received_a", i128v(11)),
            ("actual_received_b", i128v(20)),
            ("sender", addr(ROUTER)),
            ("token_a", addr(PYUSD)),
            ("token_b", addr(USDC_SAC)),
        ]),
    );
    let rows = soroban_pool_amount_rows(&[ev], &pools(), &sac());
    assert_eq!(
        legs(&rows),
        vec![(0, ids::contract_id(PYUSD), 11), (0, USDC, 20)]
    );
    assert!(
        rows.iter()
            .all(|r| r.event_kind == PoolEventKind::Deposit as u8)
    );
}

/// A malformed amount event and a per-field event outside a `sender` group
/// are refused (and logged), not half-written.
#[test]
fn unreadable_amount_events_write_nothing() {
    let short_trade = event(
        0,
        &[sym("trade"), addr(PYUSD), addr(USDC_SAC)],
        vec_of(&[i128v(5)]),
    );
    let orphan_field = event(1, &[string("swap"), string("offer_amount")], i128v(5));
    assert!(soroban_pool_amount_rows(&[short_trade, orphan_field], &pools(), &sac()).is_empty());
}
