//! In-process cache for `GET /v1/network/stats`.
//!
//! Backed by `moka::future::Cache` so the cold-cache path can use
//! `try_get_with` for **stampede protection**: when N concurrent
//! requests arrive on a cold key, only one runs the (async) DB query
//! and the rest wait on its result instead of fanning out N
//! round-trips. Per task 0180, this replaces a global
//! `OnceLock<Mutex<...>>` singleton plus a hand-rolled TTL check.
//!
//! `moka::future::Cache::try_get_with` accepts an `async` initialiser,
//! which lets us keep the DB fetch on the async runtime without
//! reintroducing the "lock held across `.await`" footgun the previous
//! impl had to guard against.
//!
//! ## Version-keyed on `latest_ledger_sequence` (task 0291)
//!
//! The cache key is the **chain head** (`ledgers.sequence`), not a unit
//! singleton on a TTL. Per request the handler does a cheap head read
//! (`crate::common::head` — `SELECT max(sequence)`, a primary-key probe,
//! see that module) and looks up under that sequence:
//!
//! - head unchanged → **HIT**, the stored snapshot is returned;
//! - head advanced → **MISS** on the new key → the stats statement runs
//!   once (under `try_get_with`) and the result is stored under the new
//!   sequence.
//!
//! So a new ledger is visible on the **first** request after it is written
//! — there is no up-to-TTL window where the previous head is still served.
//! This replaces the earlier pure-4s-TTL design, whose refresh was not
//! phase-locked to ledger arrival and could serve a state up to ~4 s behind
//! the DB. The cache's job is still purely to collapse the per-ledger client
//! fan-out into ≤1 stats query per head (stampede protection), not to extend
//! data lifetime; the response ships `Cache-Control: max-age=0`
//! (`cache_control::LIVE`) so this in-process layer is the only one.
//!
//! ## Bounding: capacity + backstop TTL
//!
//! Only the current head key is ever read; a key goes dead the instant the
//! head advances. `max_capacity` keeps just a handful of entries (the live
//! head plus any in-flight just across a ledger boundary), and a generous
//! `time_to_live` is a **backstop** — it reclaims dead keys on an idle
//! container and self-heals the pathological case of a head that never
//! advances (e.g. ingestion stalled) so a stale snapshot cannot live
//! forever. Neither bound is the freshness mechanism; version-keying is.

use std::sync::Arc;
use std::time::Duration;

use super::dto::NetworkStats;

/// Backstop TTL. Not the freshness mechanism (version-keying is) — it only
/// reclaims dead head keys on an idle container and caps how long a snapshot
/// can survive if the head stops advancing. Comfortably above the ~5.8 s
/// ledger cadence so it never races a normal head bump.
const BACKSTOP_TTL: Duration = Duration::from_secs(60);

/// At most a handful of head keys: the live head plus any in-flight across a
/// ledger boundary. Older keys are dead on head advance; capacity eviction
/// (approximate, TinyLFU) plus the backstop TTL reclaim them.
const MAX_ENTRIES: u64 = 8;

/// Cache key is the chain head (`latest_ledger_sequence`); the value is the
/// assembled stats snapshot computed at that head. `try_get_with(head)`
/// deduplicates concurrent misses on the same head down to one DB query.
pub type NetworkStatsCache = moka::future::Cache<i64, Arc<NetworkStats>>;

/// Build a fresh version-keyed cache instance.
pub fn new_network_cache() -> NetworkStatsCache {
    moka::future::Cache::builder()
        .time_to_live(BACKSTOP_TTL)
        .max_capacity(MAX_ENTRIES)
        .build()
}

#[cfg(test)]
mod tests {
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
}
