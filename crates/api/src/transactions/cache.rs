//! In-process cache for the E3 (`GET /v1/transactions/:hash`) heavy block.
//!
//! Backed by `moka::future::Cache` so the cold-cache path can use
//! `try_get_with` for **stampede protection**: when N concurrent requests
//! hit the same uncached tx hash, only one runs the (async) public-archive
//! fetch + XDR parse and the rest await its result instead of fanning out N
//! cross-region S3 GETs. This mirrors `network::cache` (task 0180).
//!
//! ## Why this cache exists (task 0330)
//!
//! The heavy block is re-derived per request by `StellarArchiveFetcher::
//! fetch_ledger` — an **uncached, cross-region** S3 GET of the whole ledger
//! `.xdr.zst` (the API Lambda runs in eu-central-1; the public Stellar
//! archive bucket is in us-east-2), followed by zstd-decompress + XDR
//! deserialize of the entire ledger batch. Measured ~1–2.7 s. A finalized
//! transaction's heavy fields are **immutable** (Stellar consensus), so any
//! repeat view / refresh of the same hash can be served from this warm-Lambda
//! cache, skipping both the S3 round-trip and the parse.
//!
//! ## Keyed by tx hash, value is the extracted heavy block (not the ledger)
//!
//! The value is the small per-tx `E3HeavyFields` (a few KB to tens of KB),
//! NOT the whole parsed `LedgerCloseMeta` (~1.5 MB decompressed). With the
//! API Lambda at **256 MB** (`infra/envs/production.json`), caching whole
//! ledgers would blow the memory budget at even a handful of entries; caching
//! the extracted slice is memory-safe AND also skips the per-request re-parse.
//! The trade-off: two distinct txs in the same ledger each pay one fetch — an
//! accepted cost, since the explorer's hot pattern is repeat views of the
//! *same* tx, not sweeps across a ledger.
//!
//! ## Bounded by bytes, not entry count
//!
//! Heavy blocks vary by orders of magnitude — a plain payment is a few KB, a
//! Soroban tx with full event topics/data + a deep invocation tree can reach
//! hundreds of KB. A fixed *entry* cap would therefore let a burst of large
//! Soroban detail views push the 256 MB Lambda toward OOM while a cap of the
//! same number of plain payments stays tiny. So the cache is bounded by an
//! approximate **byte budget** via a moka `weigher` ([`approx_heavy_bytes`]):
//! large entries naturally evict more small ones and the resident footprint is
//! capped regardless of the size mix. When the budget is exceeded, TinyLFU
//! evicts the coldest entries; an evicted/missing key just falls back to the
//! normal archive fetch — the pre-cache behaviour, never an error.
//!
//! ## Only successes are cached
//!
//! The `try_get_with` initialiser returns `Err(())` when the archive fetch
//! fails or the ledger does not contain the tx (graceful-degradation `None`).
//! moka does not cache `Err`, so an `unavailable` heavy block is never pinned
//! — a later request retries the archive (matching the `SHORT` cache-control
//! the handler already attaches to degraded responses). `parse_error` txs are
//! short-circuited by the handler before reaching this cache.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::runtime_enrichment::stellar_archive::dto::{E3HeavyFields, XdrEventDto};

/// Lifetime of a cached heavy block. The data is immutable, so this is a
/// pure memory backstop (reclaim on an idle container), not a freshness
/// mechanism — comfortably long so hot transactions stay warm across a
/// browsing session.
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

/// Total byte budget for resident heavy blocks (the `weigher` unit).
/// [`approx_heavy_bytes`] charges both string payload AND a per-node allocation
/// constant ([`JSON_NODE_BYTES`]), so the weight tracks the real resident cost
/// of the `serde_json::Value` tree rather than just its serialized length —
/// the failure mode for a scalar/many-key-dominated tree, whose nodes cost far
/// more in RAM than in content bytes. The budget is held to a small fraction of
/// the 256 MB Lambda so it stays safe alongside the ~1.5 MB parsed-ledger
/// transient and the per-hit deep clone.
///
/// moka never retains an entry whose own weight exceeds `max_capacity`, so a
/// (practically impossible) heavy block above this budget is simply never
/// cached → it refetches every time. That degrades safely (no error), it just
/// forgoes the cache for that one outlier.
const CACHE_WEIGHT_BUDGET: u64 = 12 * 1024 * 1024;

/// Approximate resident cost of a single `serde_json::Value` node (the enum
/// itself + its allocation bookkeeping). Charged per array element and per
/// object entry so a tree of many small/scalar nodes is weighted by its real
/// footprint, not by its near-zero serialized length.
const JSON_NODE_BYTES: usize = 24;

/// Cache key is the lowercase-hex tx hash; value is the extracted heavy block.
/// `try_get_with(hash)` deduplicates concurrent misses on the same hash down
/// to one archive fetch + parse.
pub type TxHeavyCache = moka::future::Cache<String, Arc<E3HeavyFields>>;

/// Build a fresh cache instance with the canonical TTL + byte budget.
pub fn new_tx_heavy_cache() -> TxHeavyCache {
    moka::future::Cache::builder()
        .time_to_live(CACHE_TTL)
        .weigher(|_hash, heavy: &Arc<E3HeavyFields>| {
            // moka weights are u32; a heavy block never approaches 4 GiB, but
            // saturate rather than wrap on the pathological case.
            u32::try_from(approx_heavy_bytes(heavy)).unwrap_or(u32::MAX)
        })
        .max_capacity(CACHE_WEIGHT_BUDGET)
        .build()
}

/// Approximate the resident size (bytes) of a heavy block for the weigher.
/// Counts string payloads + JSON value content recursively AND charges
/// [`JSON_NODE_BYTES`] per value node (array element / object entry), so the
/// weight reflects real in-memory cost: long event `topics`/`data` and
/// operation `details`/`operation_tree` are captured by their content length,
/// while a tree of many small/scalar nodes — whose serialized length is near
/// zero but whose allocations dominate RAM — is captured by the per-node
/// charge. Approximate by design; the conservative `CACHE_WEIGHT_BUDGET`
/// absorbs the remaining gap.
fn approx_heavy_bytes(h: &E3HeavyFields) -> usize {
    fn opt_len(s: &Option<String>) -> usize {
        s.as_ref().map_or(0, String::len)
    }
    fn value_size(v: &Value) -> usize {
        // Every node carries the per-node allocation cost; content is added on
        // top for the variants that own a payload.
        JSON_NODE_BYTES
            + match v {
                Value::Null | Value::Bool(_) | Value::Number(_) => 0,
                Value::String(s) => s.len(),
                Value::Array(a) => a.iter().map(value_size).sum::<usize>(),
                Value::Object(o) => o
                    .iter()
                    .map(|(k, val)| k.len() + JSON_NODE_BYTES + value_size(val))
                    .sum::<usize>(),
            }
    }
    fn event_size(e: &XdrEventDto) -> usize {
        e.event_type.len()
            + e.contract_id.as_ref().map_or(0, String::len)
            + e.topics.iter().map(value_size).sum::<usize>()
            + value_size(&e.data)
            + 8
    }

    // Fixed base covers the key string + struct overhead so tiny heavy blocks
    // still carry a non-zero weight (else thousands could pile up unbounded).
    let base = 128;
    let strings = opt_len(&h.memo_type)
        + opt_len(&h.memo)
        + opt_len(&h.fee_bump_source)
        + opt_len(&h.envelope_xdr)
        + opt_len(&h.result_xdr)
        + opt_len(&h.result_code);
    let sigs: usize = h
        .signatures
        .iter()
        .map(|s| s.hint.len() + s.signature.len())
        .sum();
    let events: usize = h.diagnostic_events.iter().map(event_size).sum::<usize>()
        + h.contract_events.iter().map(event_size).sum::<usize>();
    let ops: usize = h
        .operations
        .iter()
        .map(|o| o.op_type.len() + value_size(&o.details) + 8)
        .sum();
    let tree = h.operation_tree.as_ref().map_or(0, value_size);

    base + strings + sigs + events + ops + tree
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> E3HeavyFields {
        E3HeavyFields {
            memo_type: None,
            memo: None,
            signatures: Vec::new(),
            fee_bump_source: None,
            envelope_xdr: Some("AAAA".into()),
            result_xdr: Some("AAAB".into()),
            diagnostic_events: Vec::new(),
            contract_events: Vec::new(),
            operations: Vec::new(),
            result_code: Some("txSuccess".into()),
            operation_tree: None,
        }
    }

    /// Smoke test for the wiring of the type alias + builder. TTL expiry,
    /// eviction and concurrency are moka's responsibility.
    #[tokio::test]
    async fn miss_then_hit() {
        let cache = new_tx_heavy_cache();
        let hash = "ab".repeat(32);
        assert!(cache.get(&hash).await.is_none());
        cache.insert(hash.clone(), Arc::new(sample())).await;
        let hit = cache.get(&hash).await.expect("hit");
        assert_eq!(hit.result_code.as_deref(), Some("txSuccess"));
    }

    /// The weigher gives every block a non-zero base weight and grows with the
    /// large JSON value fields (event topics/data) — the load-bearing property
    /// for a byte-bounded cache that must not be fooled by a big Soroban block.
    #[test]
    fn weigher_grows_with_event_payload() {
        use crate::runtime_enrichment::stellar_archive::dto::XdrEventDto;
        use serde_json::json;

        let small = approx_heavy_bytes(&sample());
        assert!(small > 0, "every block carries a non-zero base weight");

        let mut big = sample();
        big.contract_events = vec![XdrEventDto {
            event_type: "contract".into(),
            contract_id: Some("C".repeat(56)),
            topics: vec![json!("x".repeat(4096)), json!("y".repeat(4096))],
            data: json!({ "blob": "z".repeat(8192) }),
            event_index: 0,
        }];
        let big_weight = approx_heavy_bytes(&big);
        assert!(
            big_weight > small + 16_000,
            "event topics/data must dominate the weight: {big_weight} vs {small}"
        );
    }

    /// Pins the per-node accounting specifically: a tree of many near-zero-
    /// content scalar nodes must still be weighted by node COUNT, not by its
    /// (tiny) serialized length. A content-only weigher would weigh this ~0 and
    /// let a burst of such blocks blow the budget — the exact OOM hole the
    /// per-node charge closes. Asserting on `JSON_NODE_BYTES * N` would silently
    /// pass under the old content-only weigher.
    #[test]
    fn weigher_charges_per_node_for_scalar_heavy_trees() {
        use serde_json::json;

        let small = approx_heavy_bytes(&sample());

        let mut scalar_heavy = sample();
        // 1000 number nodes, ~no string content: only the per-node charge can
        // grow this. `json!` of a 1000-element u8 vec is an array of 1000 nums.
        scalar_heavy.operation_tree = Some(json!(vec![0u8; 1000]));

        let weight = approx_heavy_bytes(&scalar_heavy);
        assert!(
            weight >= small + 1000 * JSON_NODE_BYTES,
            "per-node charge must weight a scalar-heavy tree: {weight} vs {small}"
        );
    }

    /// `try_get_with` must NOT cache an `Err` initialiser (degraded fetch):
    /// the next call re-runs the initialiser so a transient archive failure
    /// can recover on retry.
    #[tokio::test]
    async fn err_initializer_is_not_cached() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = new_tx_heavy_cache();
        let calls = Arc::new(AtomicUsize::new(0));
        let hash = "cd".repeat(32);

        for _ in 0..2 {
            let calls = Arc::clone(&calls);
            let res: Result<Arc<E3HeavyFields>, Arc<()>> = cache
                .try_get_with(hash.clone(), async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err::<Arc<E3HeavyFields>, ()>(())
                })
                .await;
            assert!(res.is_err());
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "Err must not be cached — both calls run the initializer"
        );
    }
}
