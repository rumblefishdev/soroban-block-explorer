use super::*;

fn chain(expect: i64, attempt: u32) -> Wake {
    Wake::Chain(Expect { expect, attempt })
}

fn stored(persisted: u64, newest: i64) -> Reconciled {
    Reconciled { persisted, newest }
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
    assert_eq!(Wake::parse(Some(r#"{"keepalive":true}"#)), Wake::Keepalive);
    assert_eq!(Wake::parse(Some(r#"{"Records":[]}"#)), Wake::Keepalive);
    assert_eq!(Wake::parse(Some("doorbell")), Wake::Keepalive);
    assert_eq!(Wake::parse(None), Wake::Keepalive);
}

#[test]
fn a_wake_that_stored_ledgers_expects_the_next_three_seconds_after_its_close() {
    // Ledger 100 closed at 1_000; 101 should close at 1_005, its file is
    // looked for at 1_008.
    let next = next(chain(100, 4), stored(1, 100), Some(1_000), 1_002).unwrap();
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
    let behind = next(chain(100, 0), stored(5, 104), Some(1_000), 5_000).unwrap();
    assert_eq!(behind.delay_secs, 1);
    // A close time in the future (clock skew) cannot park the chain.
    let ahead = next(chain(100, 0), stored(1, 100), Some(9_000), 1_000).unwrap();
    assert_eq!(ahead.delay_secs, 15);
}

#[test]
fn a_late_file_is_looked_for_again_after_1_2_4_8_then_15_seconds() {
    let delays: Vec<i32> = (0..7)
        .map(|attempt| {
            let n = next(chain(101, attempt), stored(0, 100), None, 1_000).unwrap();
            assert_eq!(n.message.expect, 101);
            assert_eq!(n.message.attempt, attempt + 1);
            n.delay_secs
        })
        .collect();
    assert_eq!(delays, vec![1, 2, 4, 8, 15, 15, 15]);
}

#[test]
fn a_chain_message_another_chain_already_served_is_dropped() {
    assert_eq!(next(chain(100, 0), stored(0, 100), None, 1_000), None);
    assert_eq!(next(chain(99, 3), stored(0, 100), None, 1_000), None);
}

#[test]
fn a_keepalive_starts_a_chain_only_when_it_stored_ledgers() {
    let started = next(Wake::Keepalive, stored(2, 100), Some(1_000), 1_002).unwrap();
    assert_eq!(started.message.expect, 101);
    assert_eq!(next(Wake::Keepalive, stored(0, 100), None, 1_002), None);
}

#[test]
fn an_empty_database_queues_nothing() {
    assert_eq!(next(Wake::Keepalive, stored(0, 0), None, 1_000), None);
    assert_eq!(next(chain(1, 0), stored(0, 0), None, 1_000), None);
}
