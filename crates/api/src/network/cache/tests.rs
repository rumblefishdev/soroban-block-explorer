use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn sample(seq: i64) -> NetworkStats {
    use chrono::TimeZone;
    let ts = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    NetworkStats {
        tps_60s: 1.5,
        total_accounts: 100,
        total_contracts: 5,
        latest_ledger_sequence: seq,
        latest_ledger_closed_at: Some(ts),
        generated_at: ts,
    }
}

#[tokio::test]
async fn put_then_get_round_trips_under_head_key() {
    let cache = new_network_cache();
    cache.insert(42, Arc::new(sample(42))).await;
    let read = cache.get(&42).await.expect("cache populated for head 42");
    assert_eq!(read.latest_ledger_sequence, 42);
    assert_eq!(read.total_accounts, 100);
}

/// Version-keying contract: an unchanged head is a HIT (no recompute),
/// an advanced head is a MISS (exactly one recompute). Sequential, so a
/// single-threaded runtime is fine.
#[tokio::test]
async fn hit_on_unchanged_head_recompute_on_advance() {
    let cache = new_network_cache();
    let computes = Arc::new(AtomicUsize::new(0));

    // Mirrors the handler: look up under `head`, computing on miss.
    async fn get(
        cache: &NetworkStatsCache,
        computes: &Arc<AtomicUsize>,
        head: i64,
    ) -> Arc<NetworkStats> {
        let computes = computes.clone();
        cache
            .try_get_with(head, async move {
                computes.fetch_add(1, Ordering::SeqCst);
                Ok::<_, std::convert::Infallible>(Arc::new(sample(head)))
            })
            .await
            .expect("infallible initializer")
    }

    // First request at head 1 → MISS → 1 compute.
    let a = get(&cache, &computes, 1).await;
    assert_eq!(a.latest_ledger_sequence, 1);
    assert_eq!(computes.load(Ordering::SeqCst), 1);

    // Same head again → HIT → no further compute.
    let b = get(&cache, &computes, 1).await;
    assert_eq!(b.latest_ledger_sequence, 1);
    assert_eq!(
        computes.load(Ordering::SeqCst),
        1,
        "unchanged head must HIT"
    );

    // Head advances to 2 → MISS → exactly one more compute.
    let c = get(&cache, &computes, 2).await;
    assert_eq!(c.latest_ledger_sequence, 2);
    assert_eq!(
        computes.load(Ordering::SeqCst),
        2,
        "advanced head must recompute"
    );
}

/// Stampede protection: concurrent misses on the same head collapse to a
/// single recompute. This MUST run on a multi-thread runtime with the
/// callers genuinely in flight at once — a barrier releases all three into
/// `try_get_with` together and the initializer yields, so the two losers
/// observe the winner's in-flight init and coalesce. A single-threaded
/// runtime or a synchronous initializer would let the first future insert
/// before the others are polled, making `computes == 1` hold by ordering
/// even if single-flight were broken — i.e. not load-bearing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_misses_on_same_head_collapse_to_one_compute() {
    use std::time::Duration;

    let cache = new_network_cache();
    let computes = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(tokio::sync::Barrier::new(3));

    let handles: Vec<_> = (0..3)
        .map(|_| {
            let cache = cache.clone();
            let computes = Arc::clone(&computes);
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                // Line all three callers up so they hit the same cold key
                // concurrently.
                gate.wait().await;
                cache
                    .try_get_with(7i64, async move {
                        computes.fetch_add(1, Ordering::SeqCst);
                        // Widen the in-flight window so the other two
                        // callers reach `try_get_with` while this init runs.
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok::<_, std::convert::Infallible>(Arc::new(sample(7)))
                    })
                    .await
                    .expect("infallible initializer")
            })
        })
        .collect();

    for h in handles {
        let stats = h.await.expect("task joined");
        assert_eq!(stats.latest_ledger_sequence, 7);
    }
    assert_eq!(
        computes.load(Ordering::SeqCst),
        1,
        "concurrent misses on the same head must collapse to one compute"
    );
}
