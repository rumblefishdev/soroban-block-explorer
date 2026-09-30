use super::*;

const TIP: Tip = Tip {
    sequence: 100,
    closed_at: 1_000,
    interval: 5,
};

fn chain(expect: i64, attempt: u32) -> Wake {
    Wake::Chain(Expect { expect, attempt })
}

#[test]
fn a_chain_message_round_trips_and_anything_else_is_a_keepalive() {
    let body = serde_json::to_string(&Expect {
        expect: 7,
        attempt: 2,
    })
    .unwrap();
    assert_eq!(body, r#"{"expect":7,"attempt":2}"#);
    assert_eq!(Wake::parse(Some(&body)), chain(7, 2));
    assert_eq!(Wake::parse(Some(r#"{"expect":7}"#)), chain(7, 0));
    assert_eq!(Wake::parse(Some(r#"{"keepalive":true}"#)), Wake::Keepalive);
    assert_eq!(Wake::parse(Some(r#"{"Records":[]}"#)), Wake::Keepalive);
    assert_eq!(Wake::parse(Some("doorbell")), Wake::Keepalive);
    assert_eq!(Wake::parse(None), Wake::Keepalive);
}

#[test]
fn a_chain_message_for_a_stored_ledger_is_dropped() {
    assert!(already_served(chain(100, 0), 100));
    assert!(already_served(chain(99, 3), 100));
    assert!(!already_served(chain(101, 0), 100));
    assert!(!already_served(Wake::Keepalive, 100));
}

#[test]
fn a_served_chain_expects_the_next_ledger_three_seconds_after_its_close() {
    // Next close at 1_005; its file is looked for at 1_008.
    let next = next(chain(100, 4), 99, Some(TIP), 1_002).unwrap();
    assert_eq!(
        next.message,
        Expect {
            expect: 101,
            attempt: 0
        }
    );
    assert_eq!(next.delay_secs, 6);
}

#[test]
fn the_delay_stays_between_one_and_fifteen_seconds() {
    // Behind (a backlog): look again at once.
    assert_eq!(
        next(chain(100, 0), 99, Some(TIP), 5_000)
            .unwrap()
            .delay_secs,
        1
    );
    // A long interval between the last two ledgers cannot park the chain.
    let slow = Tip {
        interval: 600,
        ..TIP
    };
    assert_eq!(
        next(chain(100, 0), 99, Some(slow), 1_000)
            .unwrap()
            .delay_secs,
        15
    );
}

#[test]
fn the_interval_follows_the_network_and_defaults_to_five_seconds() {
    let fast = Tip { interval: 2, ..TIP };
    assert_eq!(
        next(chain(100, 0), 99, Some(fast), 1_000)
            .unwrap()
            .delay_secs,
        5
    );
    let single = Tip { interval: 0, ..TIP };
    assert_eq!(
        next(chain(100, 0), 99, Some(single), 1_000)
            .unwrap()
            .delay_secs,
        8
    );
}

#[test]
fn a_late_file_is_looked_for_again_after_1_2_4_8_then_15_seconds() {
    let delays: Vec<i32> = (0..7)
        .map(|attempt| {
            let n = next(chain(101, attempt), 100, Some(TIP), 1_000).unwrap();
            assert_eq!(n.message.expect, 101);
            assert_eq!(n.message.attempt, attempt + 1);
            n.delay_secs
        })
        .collect();
    assert_eq!(delays, vec![1, 2, 4, 8, 15, 15, 15]);
}

#[test]
fn a_keepalive_starts_a_chain_only_when_it_found_new_ledgers() {
    let started = next(Wake::Keepalive, 98, Some(TIP), 1_002).unwrap();
    assert_eq!(started.message.expect, 101);
    assert_eq!(next(Wake::Keepalive, 100, Some(TIP), 1_002), None);
}

#[test]
fn an_empty_database_queues_nothing() {
    assert_eq!(next(Wake::Keepalive, 0, None, 1_000), None);
    assert_eq!(next(chain(1, 0), 0, None, 1_000), None);
}
