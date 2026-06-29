//! ClickHouse pilot — connection layer for the parallel store.
//!
//! See ADR 0044 (`lore/2-adrs/0044_clickhouse-pilot-parallel-store.md`) for
//! the design and task 0204 for the implementation context. This crate is
//! deliberately self-contained: no other crate in the workspace depends on
//! it, and it does not depend on `crates/db`.

use clickhouse::Client;

pub mod persist;

#[cfg(feature = "aws-mtls")]
pub mod mtls;

/// Schema embedded at compile time. Applied verbatim by `db-clickhouse-init`
/// and re-exported here so callers can apply it programmatically (e.g. from
/// integration tests).
pub const INIT_SQL: &str = include_str!("../schema/init.sql");

/// Default ClickHouse HTTP endpoint when `CLICKHOUSE_URL` is not set.
pub const DEFAULT_URL: &str = "http://localhost:8123";

/// Default ClickHouse user when `CLICKHOUSE_USER` is not set.
///
/// Matches the local-dev posture of the Postgres service: no production
/// secrets in the compose file.
pub const DEFAULT_USER: &str = "default";

/// Production CH database — `schema/init.sql` creates every table here.
/// Shared so the api / indexer / backfill-runner Lambdas all reach the
/// same logical store under mTLS.
pub const PROD_DATABASE: &str = "default";

/// Configuration for a ClickHouse client, sourced from environment variables
/// with safe local-dev defaults.
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl Config {
    /// Read from `CLICKHOUSE_URL`, `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`,
    /// `CLICKHOUSE_DATABASE`. Each falls back to the `DEFAULT_*` constants.
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string()),
            user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| DEFAULT_USER.to_string()),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default(),
            database: std::env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".into()),
        }
    }
}

/// Build a `clickhouse::Client` from a `Config`.
///
/// Clients are cheap to clone — clone instead of rebuilding so the underlying
/// hyper connection pool is reused.
pub fn client(cfg: &Config) -> Client {
    Client::default()
        .with_url(&cfg.url)
        .with_user(&cfg.user)
        .with_password(&cfg.password)
        .with_database(&cfg.database)
}

/// Errors that can happen while applying the schema or persisting a ledger.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// Underlying CH transport / driver failure (`clickhouse::error::Error`
    /// wraps HTTP, serialization, and server-reported issues).
    #[error("clickhouse query failed: {0}")]
    Query(#[from] clickhouse::error::Error),
    /// Pre-write staging failure (decode error, overflow, malformed
    /// parser output, …). Mirrors the `HandlerError::Staging` variant
    /// used by the PG path so the operator sees the same vocabulary
    /// regardless of target.
    #[error("clickhouse staging failed: {0}")]
    Staging(String),
}

/// Apply `init.sql` to the given client. Idempotent: every statement is a
/// `CREATE … IF NOT EXISTS`, so running twice is a no-op.
///
/// The HTTP query endpoint accepts one statement per request, so we split
/// the file on `;` and submit each statement individually. The DDL has no
/// embedded semicolons (no string literals, no functions), so naive
/// splitting is safe — keep the schema in that style.
pub async fn apply_init_sql(client: &Client) -> Result<(), SchemaError> {
    for stmt in split_statements(INIT_SQL) {
        client.query(&stmt).execute().await?;
    }
    Ok(())
}

/// Split a multi-statement SQL string into individual statements.
///
/// Strips line comments (`-- …`) and empty statements. Does not handle
/// quoted strings or block comments — the caller must keep `init.sql`
/// free of either.
fn split_statements(sql: &str) -> Vec<String> {
    let stripped: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");

    stripped
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_statements_drops_line_comments_and_empty_chunks() {
        let sql = "-- top comment\n\
                   CREATE TABLE a (x Int64) ENGINE = MergeTree ORDER BY x;\n\
                   -- mid comment\n\
                   CREATE TABLE b (y Int64) ENGINE = MergeTree ORDER BY y;\n\
                   ;\n";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE TABLE a"));
        assert!(stmts[1].starts_with("CREATE TABLE b"));
    }

    #[test]
    fn init_sql_parses_into_statements() {
        let stmts = split_statements(INIT_SQL);
        // 27 CREATE TABLE + 2 CREATE MATERIALIZED VIEW + 1 CREATE DICTIONARY = 30.
        // 17-table base; task 0217 added `nfts_pending` + `nft_ownership_pending`
        // as schema-only landing zones (the CH writer does NOT yet stage/INSERT
        // into either — follow-up to PR #180); task 0231 (ADR 0050) added the
        // `asset_enrichment` + `nft_enrichment` side tables; lore-0293 added the
        // `asset_aggregates` table + its refreshable `asset_aggregates_mv` for
        // pre-computed asset aggregates (`assets.total_supply` / `holder_count`
        // now served from `asset_aggregates`; the dead columns' drop is deferred
        // to task 0310); task 0297 added the `soroban_contract_metadata` side
        // table (on-chain token name/symbol/decimals); task 0331 added the
        // `soroban_token_balances` per-holder side table (asset_type=3 balances
        // from ContractData `Balance(Address)` ledger state) + the
        // task 0331 Option C: unified `addresses` + `balances` + `balance_aggregates`
        // (+ its refreshable MV) REPLACED the interim `soroban_token_balances` +
        // `soroban_asset_aggregates` (+ MV) — net one fewer table, same MV count.
        // Task 0331 step 7 added `soroban_token_supply` (authoritative per-token
        // total_supply from the instance `TotalSupply` key; the assets read
        // coalesces it over `balance_aggregates`).
        assert_eq!(
            stmts.len(),
            30,
            "expected 27 tables + 2 materialized views + 1 dictionary, got {}",
            stmts.len()
        );
    }
}
