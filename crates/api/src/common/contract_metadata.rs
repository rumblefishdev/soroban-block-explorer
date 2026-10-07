//! A Soroban contract's on-chain metadata (name, symbol, decimals) — the ONE
//! read of `soroban_contract_metadata`, shared by every endpoint that shows it.
//!
//! Every endpoint shows the same name, symbol and decimals for a contract,
//! because every endpoint reads them here. `tests/sql_conventions.rs` fails
//! when a query reads the table directly.

/// The newest metadata row per contract, as a subquery to join or select from:
/// columns `contract_id`, `name`, `symbol`, `decimals`.
///
/// Every row is a whole snapshot of the contract's `METADATA` at one ledger
/// (`init.sql`), so the newest row is the current metadata, `NULL`s included —
/// a field the newest row lacks is unpublished, never the older value.
/// `FINAL` collapses unmerged versions; the table holds ~4k rows, so it is
/// cheap at every call site.
pub const CONTRACT_METADATA: &str =
    "(SELECT contract_id, name, symbol, decimals FROM soroban_contract_metadata FINAL)";

#[cfg(test)]
mod tests;
