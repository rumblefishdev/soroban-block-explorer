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
mod tests;
