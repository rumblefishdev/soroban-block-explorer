//! Unit tests for [`super`] (the LP DTOs) — tests live in their own
//! file per the repo's test-extraction convention (task 0374).
mod pool_event_tests {
    use super::super::PoolEvent;

    /// The classifier itself. It used to live in SQL as a `multiIf` and could
    /// only be checked against a live ClickHouse; in Rust it is the one thing
    /// this endpoint gets wrong most visibly, so it gets the table.
    #[test]
    fn sign_pair_names_the_event() {
        let cases = [
            (120, 3, PoolEvent::Deposit),
            (-4, -9, PoolEvent::Withdrawal),
            (120, -4, PoolEvent::Trade),
            (-4, 120, PoolEvent::Trade),
        ];
        for (a, b, want) in cases {
            assert_eq!(PoolEvent::from_signs(a, b), want, "({a}, {b})");
        }
    }

    /// A zero leg is not a deposit and not a withdrawal, so it falls to trade
    /// rather than to whichever branch happens to be first.
    #[test]
    fn zero_leg_is_not_a_deposit() {
        assert_eq!(PoolEvent::from_signs(0, 5), PoolEvent::Trade);
        assert_eq!(PoolEvent::from_signs(0, -5), PoolEvent::Trade);
        assert_eq!(PoolEvent::from_signs(0, 0), PoolEvent::Trade);
    }

    /// `as_param` feeds the `allowed` list a rejection returns and
    /// `from_param` reads the caller's value back, so drift between them would
    /// advertise a value the endpoint then refuses.
    #[test]
    fn filter_value_round_trips() {
        for e in [PoolEvent::Trade, PoolEvent::Deposit, PoolEvent::Withdrawal] {
            assert_eq!(PoolEvent::from_param(e.as_param()), Some(e), "{e:?}");
        }
        assert_eq!(PoolEvent::from_param("swap"), None);
        assert_eq!(PoolEvent::from_param(""), None);
    }
}
