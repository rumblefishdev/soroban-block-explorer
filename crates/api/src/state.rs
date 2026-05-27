//! Shared application state injected into every axum handler via `State<AppState>`.

use sqlx::PgPool;

use crate::contracts::cache::{ContractMetadataCache, new_contract_cache};
use crate::network::cache::{NetworkStatsCache, new_network_cache};
use crate::runtime_enrichment::RuntimeEnrichment;

/// Application-wide state. All inner types are cheaply cloneable
/// (`Arc`-backed; both `moka::sync::Cache` and `moka::future::Cache`
/// clones are refcount bumps; `reqwest::Client` is also `Arc`-backed).
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub(crate) ch: Option<clickhouse::Client>,
    /// Bundle of runtime-enrichment fetchers (S3 stellar archive + SEP-1
    /// stellar.toml). One struct so the field count on `AppState` doesn't
    /// grow per new transport, and so the grouping mirrors the
    /// `runtime_enrichment` module structure 1:1.
    pub runtime_enrichment: RuntimeEnrichment,
    /// Per-Lambda warm cache for contract detail responses (45 s TTL).
    pub contract_cache: ContractMetadataCache,
    /// Per-Lambda warm cache for the `/v1/network/stats` singleton (30 s TTL).
    pub network_cache: NetworkStatsCache,
    /// `SHA256(STELLAR_NETWORK_PASSPHRASE)`. Required to align tx_set
    /// envelopes (hash-sorted) with `tx_processing` (apply order) when
    /// re-extracting heavy fields from archive XDR.
    pub network_id: [u8; 32],
}

impl AppState {
    pub fn new(
        db: PgPool,
        ch: Option<clickhouse::Client>,
        runtime_enrichment: RuntimeEnrichment,
        network_id: [u8; 32],
    ) -> Self {
        Self {
            db,
            ch,
            runtime_enrichment,
            contract_cache: new_contract_cache(),
            network_cache: new_network_cache(),
            network_id,
        }
    }

    /// CH client for a handler module routed to the ClickHouse read path.
    /// Panics when called from a module whose env override is `ch` but the
    /// cold-start gate decided not to build the client — that combination
    /// is a misconfigured deploy and should fail loudly on first request.
    pub fn ch(&self) -> &clickhouse::Client {
        self.ch.as_ref().expect(
            "CH client not built — module routed to CH without API_DATASOURCE_*=ch at cold start",
        )
    }

    /// Test constructor — defaults the CH client to `None`, the caches
    /// to fresh empties, and the network id to mainnet. Keeps every
    /// `#[cfg(test)]` site free of edits when new fields land.
    #[cfg(test)]
    pub fn for_tests(db: PgPool, runtime_enrichment: RuntimeEnrichment) -> Self {
        Self::new(
            db,
            None,
            runtime_enrichment,
            xdr_parser::network_id(xdr_parser::MAINNET_PASSPHRASE),
        )
    }
}
