//! The chart assembly in isolation: which buckets appear and how a state is
//! priced, with the database's answers written by hand.

use chrono::{DateTime, TimeZone, Utc};

use super::{ChartInputs, assemble_chart};

fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
}

fn ms(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

fn secs(t: DateTime<Utc>) -> i64 {
    t.timestamp()
}

/// One bucket's last state, priced at `price_bucket`, from `samples` rows.
fn add_state(
    inputs: &mut ChartInputs,
    bucket: DateTime<Utc>,
    reserves: Vec<Option<f64>>,
    price_bucket: DateTime<Utc>,
    samples: u64,
) {
    inputs
        .states
        .insert(ms(bucket), vec![(secs(price_bucket), reserves)]);
    inputs.samples.insert(ms(bucket), samples);
}

fn tvls(points: &[crate::liquidity_pools::dto::ChartDataPoint]) -> Vec<Option<String>> {
    points.iter().map(|p| p.tvl.clone()).collect()
}

#[test]
fn only_buckets_in_which_the_pool_changed_or_traded_appear() {
    // Day 1 changed, day 2 was quiet, day 3 traded without a state row.
    let day1 = at(2026, 9, 1, 0);
    let day3 = at(2026, 9, 3, 0);
    let mut inputs = ChartInputs {
        prices: vec![vec![(secs(day1), 2.0)]],
        ..ChartInputs::default()
    };
    add_state(&mut inputs, day1, vec![Some(10.0)], day1, 4);
    inputs.volumes.insert(ms(day1), Some(7.0));
    inputs.volumes.insert(ms(day3), Some(3.0));

    let points = assemble_chart(&inputs, 30);

    let buckets: Vec<_> = points.iter().map(|p| p.bucket).collect();
    assert_eq!(buckets, vec![day1, day3]);
    assert_eq!(tvls(&points), vec![Some("20.00".into()), None]);
    assert_eq!(points[0].samples_in_bucket, 4);
    assert_eq!(points[0].volume.as_deref(), Some("7.00"));
    assert_eq!(points[1].samples_in_bucket, 0);
    assert_eq!(points[1].volume.as_deref(), Some("3.00"));
}

#[test]
fn a_week_prices_at_the_day_of_its_last_change() {
    // The week of Monday 2026-09-07 last changed on Wednesday; Sunday's
    // close is higher and must not be used.
    let monday = at(2026, 9, 7, 0);
    let wednesday = at(2026, 9, 9, 0);
    let mut inputs = ChartInputs {
        prices: vec![vec![
            (secs(wednesday), 5.0),
            (secs(at(2026, 9, 13, 0)), 8.0),
        ]],
        ..ChartInputs::default()
    };
    add_state(&mut inputs, monday, vec![Some(1.0)], wednesday, 2);

    let points = assemble_chart(&inputs, 30);

    assert_eq!(points[0].bucket, monday);
    assert_eq!(tvls(&points), vec![Some("5.00".into())]);
}

#[test]
fn a_week_whose_last_day_does_not_price_falls_back_to_an_earlier_day() {
    // The week of Monday 2026-08-17 changed on Monday and on Thursday. The
    // only close is Monday's: 72 h before Thursday, past the cap. The week
    // shows Monday's state, the newest one that prices.
    let monday = at(2026, 8, 17, 0);
    let thursday = at(2026, 8, 20, 0);
    let mut inputs = ChartInputs {
        prices: vec![vec![(secs(monday), 3.0)]],
        ..ChartInputs::default()
    };
    inputs.states.insert(
        ms(monday),
        vec![
            (secs(thursday), vec![Some(9.0)]),
            (secs(monday), vec![Some(5.0)]),
        ],
    );
    inputs.samples.insert(ms(monday), 6);

    let points = assemble_chart(&inputs, 30);

    assert_eq!(tvls(&points), vec![Some("15.00".into())]);
    assert_eq!(points[0].samples_in_bucket, 6);
}

#[test]
fn the_newest_day_that_prices_wins_in_any_order() {
    // Both days price; the database lists Monday before Tuesday. The week
    // shows Tuesday's state at Tuesday's close.
    let monday = at(2026, 8, 17, 0);
    let tuesday = at(2026, 8, 18, 0);
    let mut inputs = ChartInputs {
        prices: vec![vec![(secs(monday), 3.0), (secs(tuesday), 4.0)]],
        ..ChartInputs::default()
    };
    inputs.states.insert(
        ms(monday),
        vec![
            (secs(monday), vec![Some(5.0)]),
            (secs(tuesday), vec![Some(9.0)]),
        ],
    );

    let points = assemble_chart(&inputs, 30);

    assert_eq!(tvls(&points), vec![Some("36.00".into())]);
}

#[test]
fn a_price_older_than_the_carry_cap_prices_nothing() {
    // The only close is on day 1: 48 h before day 3 (inside the cap), 72 h
    // before day 4 (past it).
    let day3 = at(2026, 9, 3, 0);
    let day4 = at(2026, 9, 4, 0);
    let mut inputs = ChartInputs {
        prices: vec![vec![(secs(at(2026, 9, 1, 0)), 1.0)]],
        ..ChartInputs::default()
    };
    add_state(&mut inputs, day3, vec![Some(10.0)], day3, 1);
    add_state(&mut inputs, day4, vec![Some(10.0)], day4, 1);

    let points = assemble_chart(&inputs, 30);

    assert_eq!(tvls(&points), vec![Some("10.00".into()), None]);
}

#[test]
fn a_leg_without_a_reserve_or_a_price_leaves_no_tvl() {
    let day = at(2026, 9, 1, 0);
    let prices = vec![vec![(secs(day), 1.0)], vec![(secs(day), 1.0)]];
    let mut unknown_reserve = ChartInputs {
        prices: prices.clone(),
        ..ChartInputs::default()
    };
    add_state(&mut unknown_reserve, day, vec![Some(10.0), None], day, 1);
    let mut unpriced_leg = ChartInputs {
        prices: vec![prices[0].clone(), Vec::new()],
        ..ChartInputs::default()
    };
    add_state(&mut unpriced_leg, day, vec![Some(10.0), Some(1.0)], day, 1);
    let mut short_reserves = ChartInputs {
        prices,
        ..ChartInputs::default()
    };
    add_state(&mut short_reserves, day, vec![Some(10.0)], day, 1);

    for inputs in [unknown_reserve, unpriced_leg, short_reserves] {
        let points = assemble_chart(&inputs, 30);
        assert_eq!(tvls(&points), vec![None], "never a partial sum");
    }
}

#[test]
fn a_pool_with_no_legs_has_no_tvl() {
    let day = at(2026, 9, 1, 0);
    let mut inputs = ChartInputs::default();
    add_state(&mut inputs, day, Vec::new(), day, 1);

    let points = assemble_chart(&inputs, 30);

    assert_eq!(
        tvls(&points),
        vec![None],
        "never $0.00 for an unvalued pool"
    );
}
