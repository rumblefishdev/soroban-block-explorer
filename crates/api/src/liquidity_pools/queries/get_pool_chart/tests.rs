use super::*;

fn leg(code: &str) -> PriceLeg {
    PriceLeg {
        kind: "credit",
        code: code.to_string(),
        issuer: "GISSUER".to_string(),
    }
}

fn ctx(decimals: Vec<Option<u32>>) -> PoolChartContext {
    PoolChartContext {
        price: PoolPriceContext {
            legs: vec![leg("AAA"), leg("BBB")],
            fee_bps: 30,
        },
        pool_kind: domain::PoolKind::Soroban,
        leg_decimals: decimals,
    }
}

const HOUR_MS: i64 = 3_600_000;

/// Each leg scales by its own decimals and prices at the last close at or
/// before the row's price bucket; the concentrated per-tick tail past the
/// legs is ignored.
#[test]
fn tvl_scales_each_leg_and_carries_the_last_close() {
    let c = ctx(vec![Some(7), Some(18)]);
    let (a, b) = (leg("AAA"), leg("BBB"));
    let closes = HashMap::from([
        (&a, BTreeMap::from([(0, 2.0), (10 * HOUR_MS, 3.0)])),
        (&b, BTreeMap::from([(5 * HOUR_MS, 0.5)])),
    ]);
    let reserves = [
        "10000000".into(),
        "4000000000000000000".into(),
        "999".into(),
    ];
    // At 12h: A's last close is 3.0 (10h), B's is 0.5 (5h).
    assert_eq!(
        soroban_tvl(&c, &closes, &reserves, 12 * HOUR_MS),
        Some(1.0 * 3.0 + 4.0 * 0.5)
    );
}

/// A partial sum would understate the pool while looking real: an unknown
/// scale, a missing close, or a close older than the carry cap is no TVL.
#[test]
fn tvl_is_absent_unless_every_leg_prices() {
    let (a, b) = (leg("AAA"), leg("BBB"));
    let closes = HashMap::from([
        (&a, BTreeMap::from([(0, 2.0)])),
        (&b, BTreeMap::from([(0, 1.0)])),
    ]);
    let reserves = ["10000000".into(), "10000000".into()];
    assert_eq!(
        soroban_tvl(&ctx(vec![Some(7), None]), &closes, &reserves, HOUR_MS),
        None
    );

    let only_a = HashMap::from([(&a, BTreeMap::from([(0, 2.0)]))]);
    assert_eq!(
        soroban_tvl(&ctx(vec![Some(7), Some(7)]), &only_a, &reserves, HOUR_MS),
        None
    );

    let past_cap = (MAX_PRICE_CARRY_SECONDS + 1) * 1000;
    assert_eq!(
        soroban_tvl(&ctx(vec![Some(7), Some(7)]), &closes, &reserves, past_cap),
        None
    );
    assert_eq!(
        soroban_tvl(&ctx(vec![Some(7), Some(7)]), &closes, &reserves, HOUR_MS),
        Some(3.0)
    );
}
