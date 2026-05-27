//! Runtime configuration for the API service.
//!
//! All environment variable reads happen in [`AppConfig::from_env`] so
//! that `fn app(&AppConfig) -> Router` stays pure — tests construct
//! their own `AppConfig` without touching `std::env`.

use crate::common::datasource::DataSource;

/// Application-wide runtime configuration.
///
/// The `version` advertised in the OpenAPI spec is sourced from
/// `env!("CARGO_PKG_VERSION")` directly at the `ApiDoc` derive site,
/// so it does not need to live on this struct.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Fully-qualified base URL advertised to OpenAPI clients in the
    /// `servers` block. In production this is the API Gateway custom
    /// domain (e.g. `https://api.staging.sorobanscan.rumblefish.dev`);
    /// locally it falls back to `http://localhost:9000`.
    pub base_url: String,
    /// True when at least one handler module is configured to read
    /// from ClickHouse via `API_DATASOURCE_<MODULE>=ch`. Drives the
    /// cold-start decision in `main.rs` to build (or skip) the mTLS
    /// CH client; PG-only deploys keep `false` and never touch the
    /// Secrets Lambda Extension.
    pub ch_enabled: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("API_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            ch_enabled: DataSource::any_ch_enabled(),
        }
    }
}
