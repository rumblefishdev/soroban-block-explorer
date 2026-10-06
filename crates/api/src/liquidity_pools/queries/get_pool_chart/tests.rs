//! The chart assembly in isolation: buckets, carry-forward and pricing, with
//! the database's answers written by hand.

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

/// A day long after every window below, so no bucket is "still running".
fn later() -> DateTime<Utc> {
    at(2030, 1, 1, 0)
}

fn tvls(points: &[crate::liquidity_pools::dto::ChartDataPoint]) -> Vec<Option<String>> {
    points.iter().map(|p| p.tvl.clone()).collect()
}

#[test]
fn a_quiet_pool_draws_its_last_state_in_every_bucket() {
    // No change in the window: only the state before it. Leg A at $2 and
    // leg B at $1 on each of the three days.
    let inputs = ChartInputs {
        seed: Some(vec![Some(10.0), Some(5.0)]),
        prices: vec![
            vec![
                (secs(at(2026, 9, 1, 0)), 2.0),
                (secs(at(2026, 9, 2, 0)), 2.0),
                (secs(at(2026, 9, 3, 0)), 2.0),
            ],
            vec![
                (secs(at(2026, 9, 1, 0)), 1.0),
                (secs(at(2026, 9, 2, 0)), 1.0),
                (secs(at(2026, 9, 3, 0)), 1.0),
            ],
        ],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1d",
        at(2026, 9, 1, 0),
        at(2026, 9, 4, 0),
        later(),
        &inputs,
        30,
    );

    assert_eq!(points.len(), 3);
    assert_eq!(tvls(&points), vec![Some("25.00".into()); 3]);
    // Carried, not observed: no state row, no trades.
    assert!(
        points
            .iter()
            .all(|p| p.samples_in_bucket == 0 && p.volume.is_none())
    );
}

#[test]
fn a_carried_state_is_priced_at_each_bucket_s_own_price() {
    let inputs = ChartInputs {
        seed: Some(vec![Some(10.0)]),
        prices: vec![vec![
            (secs(at(2026, 9, 1, 0)), 1.0),
            (secs(at(2026, 9, 2, 0)), 3.0),
        ]],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1d",
        at(2026, 9, 1, 0),
        at(2026, 9, 3, 0),
        later(),
        &inputs,
        30,
    );

    assert_eq!(
        tvls(&points),
        vec![Some("10.00".into()), Some("30.00".into())]
    );
}

#[test]
fn a_pool_born_in_the_window_starts_at_its_first_state() {
    // No state before the window; the first change is on day 2, then the
    // pool is quiet on day 3.
    let day2 = at(2026, 9, 2, 0);
    let inputs = ChartInputs {
        seed: None,
        states: [(ms(day2), vec![Some(4.0)])].into_iter().collect(),
        samples: [(ms(day2), 3)].into_iter().collect(),
        volumes: [(ms(day2), Some(7.0))].into_iter().collect(),
        prices: vec![vec![(secs(at(2026, 9, 1, 0)), 1.0)]],
    };
    let points = assemble_chart(
        "1d",
        at(2026, 9, 1, 0),
        at(2026, 9, 4, 0),
        later(),
        &inputs,
        30,
    );

    let buckets: Vec<_> = points.iter().map(|p| p.bucket).collect();
    assert_eq!(buckets, vec![day2, at(2026, 9, 3, 0)]);
    assert_eq!(points[0].samples_in_bucket, 3);
    assert_eq!(points[0].volume.as_deref(), Some("7.00"));
    // The flow is not carried: the quiet day traded nothing.
    assert_eq!(points[1].samples_in_bucket, 0);
    assert_eq!(points[1].volume, None);
    assert_eq!(points[1].fee_revenue, None);
    assert_eq!(points[1].tvl.as_deref(), Some("4.00"));
}

#[test]
fn a_price_older_than_the_carry_cap_prices_nothing() {
    // The last close is three days before the bucket: past the 48 h cap.
    let inputs = ChartInputs {
        seed: Some(vec![Some(10.0)]),
        prices: vec![vec![(secs(at(2026, 9, 1, 0)), 1.0)]],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1d",
        at(2026, 9, 3, 0),
        at(2026, 9, 5, 0),
        later(),
        &inputs,
        30,
    );

    // Day 3 is 48 h after the close (inside the cap), day 4 is past it.
    assert_eq!(tvls(&points), vec![Some("10.00".into()), None]);
}

#[test]
fn a_leg_without_a_reserve_or_a_price_leaves_no_tvl() {
    let prices = vec![
        vec![(secs(at(2026, 9, 1, 0)), 1.0)],
        vec![(secs(at(2026, 9, 1, 0)), 1.0)],
    ];
    let unknown_reserve = ChartInputs {
        seed: Some(vec![Some(10.0), None]),
        prices: prices.clone(),
        ..ChartInputs::default()
    };
    let unpriced_leg = ChartInputs {
        seed: Some(vec![Some(10.0), Some(1.0)]),
        prices: vec![prices[0].clone(), Vec::new()],
        ..ChartInputs::default()
    };
    for inputs in [unknown_reserve, unpriced_leg] {
        let points = assemble_chart(
            "1d",
            at(2026, 9, 1, 0),
            at(2026, 9, 2, 0),
            later(),
            &inputs,
            30,
        );
        assert_eq!(tvls(&points), vec![None], "never a partial sum");
    }
}

#[test]
fn weekly_buckets_start_on_monday_and_price_at_their_last_day() {
    // 2026-09-07 is a Monday. The window starts mid-week, on Wednesday.
    let monday = at(2026, 9, 7, 0);
    let sunday = at(2026, 9, 13, 0);
    let inputs = ChartInputs {
        seed: Some(vec![Some(1.0)]),
        prices: vec![vec![(secs(at(2026, 9, 9, 0)), 5.0), (secs(sunday), 8.0)]],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1w",
        at(2026, 9, 9, 12),
        at(2026, 9, 14, 0),
        later(),
        &inputs,
        30,
    );

    assert_eq!(points.len(), 1);
    assert_eq!(points[0].bucket, monday);
    assert_eq!(points[0].tvl.as_deref(), Some("8.00"));
}

#[test]
fn the_running_week_prices_at_today_not_at_its_sunday() {
    // `now` is Wednesday 2026-09-09: the week's Sunday has no price yet, so
    // the bucket prices at today — through the carry, at Tuesday's close.
    let inputs = ChartInputs {
        seed: Some(vec![Some(1.0)]),
        prices: vec![vec![(secs(at(2026, 9, 8, 0)), 6.0)]],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1w",
        at(2026, 9, 7, 0),
        at(2026, 9, 9, 15),
        at(2026, 9, 9, 15),
        &inputs,
        30,
    );

    assert_eq!(tvls(&points), vec![Some("6.00".into())]);
}

#[test]
fn hourly_buckets_follow_the_hour() {
    let inputs = ChartInputs {
        seed: Some(vec![Some(2.0)]),
        prices: vec![vec![
            (secs(at(2026, 9, 1, 10)), 1.0),
            (secs(at(2026, 9, 1, 11)), 2.0),
        ]],
        ..ChartInputs::default()
    };
    let from = Utc.with_ymd_and_hms(2026, 9, 1, 10, 30, 0).unwrap();
    let points = assemble_chart("1h", from, at(2026, 9, 1, 12), later(), &inputs, 30);

    let buckets: Vec<_> = points.iter().map(|p| p.bucket).collect();
    assert_eq!(buckets, vec![at(2026, 9, 1, 10), at(2026, 9, 1, 11)]);
    assert_eq!(
        tvls(&points),
        vec![Some("2.00".into()), Some("4.00".into())]
    );
}

#[test]
fn a_weekly_range_ending_mid_week_prices_its_last_week_at_its_end() {
    // The range ends Thursday 2026-09-10, long ago; closes stop the day
    // before. The week must price at Wednesday, not at its Sunday (which
    // would be past the 48 h cap from Wednesday's close).
    let inputs = ChartInputs {
        seed: Some(vec![Some(1.0)]),
        prices: vec![vec![(secs(at(2026, 9, 9, 0)), 4.0)]],
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1w",
        at(2026, 9, 7, 0),
        at(2026, 9, 10, 0),
        later(),
        &inputs,
        30,
    );

    assert_eq!(tvls(&points), vec![Some("4.00".into())]);
}

#[test]
fn nothing_is_drawn_past_now() {
    let inputs = ChartInputs {
        seed: Some(vec![Some(1.0)]),
        prices: vec![vec![(secs(at(2026, 9, 1, 0)), 1.0)]],
        ..ChartInputs::default()
    };
    // `to` a day ahead of `now` (noon on day 1): only day 1 exists.
    let points = assemble_chart(
        "1d",
        at(2026, 9, 1, 0),
        at(2026, 9, 3, 0),
        at(2026, 9, 1, 12),
        &inputs,
        30,
    );

    assert_eq!(points.len(), 1);
}

#[test]
fn a_pool_with_no_legs_has_no_tvl() {
    let inputs = ChartInputs {
        seed: Some(Vec::new()),
        ..ChartInputs::default()
    };
    let points = assemble_chart(
        "1d",
        at(2026, 9, 1, 0),
        at(2026, 9, 2, 0),
        later(),
        &inputs,
        30,
    );

    assert_eq!(
        tvls(&points),
        vec![None],
        "never $0.00 for an unvalued pool"
    );
}
