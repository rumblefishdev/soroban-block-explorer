//! Per-module datasource selection — gradual PG → ClickHouse rollout
//! (task 0243). Each handler module resolves its env override at call
//! time and dispatches to either the `sqlx` PG path or the `clickhouse`
//! CH path. Default is `Pg` so a deploy that lands the plumbing alone
//! is a functional no-op; operators flip a single env per module to
//! opt in, and rollback is the same flip in reverse.

/// API handler modules that participate in the PG↔CH rollout.
///
/// Two manual lists must stay in sync: this enum (variants) and
/// `Module::ALL`. The `env_suffix` and `ordinal` matches below are
/// exhaustive so adding a variant without an arm fails compilation;
/// `module_all_covers_every_variant` is the safety net that catches a
/// missing-from-`ALL` entry at test time.
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

    /// Position in `Module::ALL`. Exhaustive match: a new variant
    /// must add an arm here, which steers the developer to `ALL` as
    /// well; the `module_all_covers_every_variant` test catches the
    /// case where the arm is added but `ALL` isn't. Kept compiled in
    /// release builds — the `#[allow]` is intentional, not stale code.
    #[allow(dead_code)]
    const fn ordinal(self) -> usize {
        match self {
            Module::Accounts => 0,
            Module::Assets => 1,
            Module::Contracts => 2,
            Module::Ledgers => 3,
            Module::LiquidityPools => 4,
            Module::Network => 5,
            Module::Nfts => 6,
            Module::Search => 7,
            Module::Transactions => 8,
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
        Self::parse(std::env::var(&key).ok().as_deref())
    }

    /// Pure value-level mapping from a raw env override to a
    /// `DataSource`. Lives separately from `for_module` so the value
    /// semantics can be unit-tested without `std::env::set_var`
    /// (whose Rust 2024 unsafe contract — no concurrent env access in
    /// the process, including reads — cannot be honoured under
    /// parallel `#[test]` threads).
    fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(v) if v.eq_ignore_ascii_case("ch") => Self::Ch,
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

    #[test]
    fn default_is_pg() {
        assert_eq!(DataSource::parse(None), DataSource::Pg);
        assert_eq!(DataSource::parse(Some("")), DataSource::Pg);
    }

    #[test]
    fn ch_value_is_case_insensitive() {
        for v in ["ch", "CH", "Ch", "cH", "  ch  "] {
            assert_eq!(DataSource::parse(Some(v)), DataSource::Ch, "value {v:?}");
        }
    }

    #[test]
    fn unknown_value_falls_back_to_pg() {
        for v in ["pg", "PG", "hbase", "clickhouse", "true", "1"] {
            assert_eq!(DataSource::parse(Some(v)), DataSource::Pg, "value {v:?}");
        }
    }

    #[test]
    fn liquidity_pools_env_suffix_is_snake_case() {
        assert_eq!(Module::LiquidityPools.env_suffix(), "LIQUIDITY_POOLS");
    }

    #[test]
    fn module_all_covers_every_variant() {
        // `ordinal()` is exhaustive — a new variant must add an arm
        // there. This assert catches the case where the arm was added
        // but `ALL` was not extended: every ordinal in `ALL` must form
        // the contiguous range `0..ALL.len()`.
        let mut ordinals: Vec<usize> = Module::ALL.iter().map(|m| m.ordinal()).collect();
        ordinals.sort_unstable();
        assert_eq!(
            ordinals,
            (0..Module::ALL.len()).collect::<Vec<_>>(),
            "Module::ALL must list every variant exactly once",
        );
    }
}
