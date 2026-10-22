//! HTTP fetch of issuer stellar.toml files (SEP-1).
//!
//! In-process LRU-cached. Resolves an issuer's on-chain
//! `home_domain` to `https://{home_domain}/.well-known/stellar.toml`, parses the
//! TOML, and surfaces the relevant slice for downstream consumers.
//!
//! Used by the enrichment worker and the backfill runner. The API read it per
//! request until task 0655; it now serves what the worker stored.
//!
//! Reference: <https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0001.md>

mod client;
pub mod dto;
pub mod errors;

pub use client::Sep1Fetcher;
// Shared with the NFT token-uri fetcher (lore-0455): one SSRF redirect
// policy for every outbound enrichment fetch, instead of two drifting ones.
#[cfg(test)]
pub(crate) use client::redirect_decisions_for_test;
pub(crate) use client::same_etld1_redirect_policy;
pub use dto::{Sep1Currency, Sep1TomlParsed};
// `Sep1Documentation` and `Sep1Error` are intentionally accessed by their
// fully-qualified path (`sep1::dto::Sep1Documentation`, `sep1::errors::Sep1Error`)
// since no current consumer needs them re-exported. Add re-exports here when
// the second consumer materialises.
