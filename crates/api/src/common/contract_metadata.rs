//! A Soroban contract's on-chain metadata (name, symbol, decimals), read as
//! the newest row per contract, so the endpoints that show it agree.

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
