//! Per-module datasource selection — gradual PG → ClickHouse rollout
//! (task 0243). Each handler module resolves its env override at call
//! time and dispatches to either the `sqlx` PG path or the `clickhouse`
//! CH path. Default is `Pg` so a deploy that lands the plumbing alone
//! is a functional no-op; operators flip a single env per module to
//! opt in, and rollback is the same flip in reverse.

/// API handler modules that participate in the PG↔CH rollout. The variant
/// set is the single source of truth — `Module::ALL`, `any_ch_enabled`,
/// and per-handler call sites all walk this enum, so a new module gets
/// a compile error if it is not added here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    Accounts,
    Assets,
    Contracts,
    Ledgers,
    LiquidityPools,
    Network,
    Nfts,
    Search,
    Transactions,
}

impl Module {
    pub const ALL: &'static [Module] = &[
        Module::Accounts,
        Module::Assets,
        Module::Contracts,
        Module::Ledgers,
        Module::LiquidityPools,
        Module::Network,
        Module::Nfts,
        Module::Search,
        Module::Transactions,
    ];

    /// Suffix appended to `API_DATASOURCE_` to form the env-var name.
    /// Must stay uppercase + snake-case so the env names mirror the
    /// handler directory names (`liquidity_pools` → `LIQUIDITY_POOLS`).
    pub const fn env_suffix(self) -> &'static str {
        match self {
            Module::Accounts => "ACCOUNTS",
            Module::Assets => "ASSETS",
            Module::Contracts => "CONTRACTS",
            Module::Ledgers => "LEDGERS",
            Module::LiquidityPools => "LIQUIDITY_POOLS",
            Module::Network => "NETWORK",
            Module::Nfts => "NFTS",
            Module::Search => "SEARCH",
            Module::Transactions => "TRANSACTIONS",
        }
    }
}

/// Env-var prefix for every per-module override
/// (e.g. `API_DATASOURCE_NETWORK`).
const ENV_PREFIX: &str = "API_DATASOURCE_";

/// Which datastore a handler module hits for the current invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Pg,
    Ch,
}

impl DataSource {
    /// Read once per invocation, not memoised. Lambda env vars are
    /// stable across a container's life, and `std::env::var` is cheap
    /// compared to a CH HTTP round-trip — avoiding a `OnceCell` keeps
    /// tests free of process-global state that would force
    /// `--test-threads=1`.
    pub fn for_module(module: Module) -> Self {
        let key = format!("{ENV_PREFIX}{}", module.env_suffix());
        match std::env::var(&key).as_deref().map(str::trim) {
            Ok("ch") | Ok("CH") => Self::Ch,
            _ => Self::Pg,
        }
    }

    /// True iff at least one module is configured to read from CH.
    /// Used at cold start to decide whether to build the mTLS client —
    /// PG-only deploys must not pay the Secrets Lambda Extension
    /// round-trip nor require `CH_DOMAIN` / `MTLS_SECRET_NAME`.
    pub fn any_ch_enabled() -> bool {
        Module::ALL
            .iter()
            .any(|m| matches!(Self::for_module(*m), Self::Ch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_env::with_env;

    #[test]
    fn default_is_pg() {
        with_env("API_DATASOURCE_NETWORK", None, || {
            assert_eq!(DataSource::for_module(Module::Network), DataSource::Pg);
        });
    }

    #[test]
    fn ch_lowercase_selects_ch() {
        with_env("API_DATASOURCE_NETWORK", Some("ch"), || {
            assert_eq!(DataSource::for_module(Module::Network), DataSource::Ch);
        });
    }

    #[test]
    fn ch_uppercase_selects_ch() {
        with_env("API_DATASOURCE_NETWORK", Some("CH"), || {
            assert_eq!(DataSource::for_module(Module::Network), DataSource::Ch);
        });
    }

    #[test]
    fn unknown_value_falls_back_to_pg() {
        with_env("API_DATASOURCE_NETWORK", Some("hbase"), || {
            assert_eq!(DataSource::for_module(Module::Network), DataSource::Pg);
        });
    }

    #[test]
    fn liquidity_pools_env_suffix_is_snake_case() {
        assert_eq!(Module::LiquidityPools.env_suffix(), "LIQUIDITY_POOLS");
        with_env("API_DATASOURCE_LIQUIDITY_POOLS", Some("ch"), || {
            assert_eq!(
                DataSource::for_module(Module::LiquidityPools),
                DataSource::Ch
            );
        });
    }
}
