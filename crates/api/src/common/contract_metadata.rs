//! A Soroban contract's on-chain metadata (name, symbol, decimals) — the ONE
//! read of `soroban_contract_metadata`, shared by every endpoint that shows it.
//!
//! The API read this table thirteen times in seven files, deduplicated three
//! ways: `FINAL`, `argMax(x, version)` and `argMax(tuple(x), version).1`. The
//! middle one is wrong: `argMax` skips a `NULL` argument, so a newest row
//! without a name or decimals fell back to an older version's value. A fix
//! landed in one copy (#518) and not in the others; one definition cannot
//! drift that way.

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
