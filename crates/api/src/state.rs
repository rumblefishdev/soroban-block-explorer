//! Shared application state injected into every axum handler via `State<AppState>`.

use std::sync::{Arc, RwLock};

use crate::contracts::cache::{ContractMetadataCache, new_contract_cache};
use crate::network::cache::{NetworkStatsCache, new_network_cache};
use crate::network::dto::NetworkStats;
use crate::runtime_enrichment::RuntimeEnrichment;

/// Application-wide state. All inner types are cheaply cloneable
/// (`Arc`-backed; both `moka::sync::Cache` and `moka::future::Cache`
/// clones are refcount bumps; `reqwest::Client` is also `Arc`-backed).
#[derive(Clone)]
pub struct AppState {
    pub(crate) ch: clickhouse::Client,
    /// Bundle of runtime-enrichment fetchers (S3 stellar archive + SEP-1
    /// stellar.toml). One struct so the field count on `AppState` doesn't
    /// grow per new transport, and so the grouping mirrors the
    /// `runtime_enrichment` module structure 1:1.
    pub runtime_enrichment: RuntimeEnrichment,
    /// Per-Lambda warm cache for contract detail responses (45 s TTL).
    pub contract_cache: ContractMetadataCache,
    /// Per-Lambda warm cache for `/v1/network/stats`, version-keyed on the
    /// chain head (`latest_ledger_sequence`) — see `network/cache.rs`.
    pub network_cache: NetworkStatsCache,
    /// Last successfully-computed network-stats snapshot, updated on every
    /// cache miss. Serves as the availability fallback when the per-request
    /// head read fails (so a transient DB/CH blip does not 500 a request the
    /// warm cache could still answer). Written only on a miss; read only on a
    /// head-read error — both rare, so a std `RwLock` is ample.
    pub network_last_good: Arc<RwLock<Option<Arc<NetworkStats>>>>,
    /// `SHA256(STELLAR_NETWORK_PASSPHRASE)`. Required to align tx_set
    /// envelopes (hash-sorted) with `tx_processing` (apply order) when
    /// re-extracting heavy fields from archive XDR.
    pub network_id: [u8; 32],
    /// Test-only counter incremented each time a list handler actually issues
    /// its heavy list query (`fetch_list_for_source`). It exists so the
    /// conditional-GET tests can assert the load-bearing invariant that a `304`
    /// short-circuit returns **without** running the heavy query (task 0292).
    /// Compiled out of release builds.
    #[cfg(test)]
    pub list_query_count: Arc<std::sync::atomic::AtomicU64>,
}

impl AppState {
    pub fn new(
        ch: clickhouse::Client,
        runtime_enrichment: RuntimeEnrichment,
        network_id: [u8; 32],
    ) -> Self {
        Self {
            ch,
            runtime_enrichment,
            contract_cache: new_contract_cache(),
            network_cache: new_network_cache(),
            network_last_good: Arc::new(RwLock::new(None)),
            network_id,
            #[cfg(test)]
            list_query_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// CH client for the handler read path.
    ///
    /// Returns an owned client (clone — cheap, the `hyper` pool is
    /// `Arc`-shared) rather than a borrow, so the load-test correlation can
    /// stamp it per request: when the [`crate::common::request_id`] capture
    /// scope holds an `X-Request-Id` (only possible under `load_testing` — the
    /// layer is wired only then), every query from the returned client carries
    /// `log_comment=<id>`, letting `system.query_log` be JOINed back to the
    /// exact HTTP request (task 0338, "B2"). No scope (normal production) → the
    /// clone is returned unmodified.
    pub fn ch(&self) -> clickhouse::Client {
        let client = self.ch.clone();
        match crate::common::request_id::current() {
            Some(id) => client.with_setting("log_comment", id),
            None => client,
        }
    }

    /// Test constructor — caller supplies the CH client (an unconnected
    /// `clickhouse::Client::default()` for validation-only tests, or a real
    /// `CLICKHOUSE_URL` client for the DB-backed conditional-GET tests). Caches
    /// default to fresh empties, network id to mainnet. Keeps every
    /// `#[cfg(test)]` site free of edits when new fields land.
    #[cfg(test)]
    pub fn for_tests(ch: clickhouse::Client, runtime_enrichment: RuntimeEnrichment) -> Self {
        Self::new(
            ch,
            runtime_enrichment,
            xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE),
        )
    }
}
