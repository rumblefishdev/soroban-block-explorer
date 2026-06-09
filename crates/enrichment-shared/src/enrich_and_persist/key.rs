//! Composite keys for the enrichment side tables — the ClickHouse ORDER BY
//! tuples (CH has no surrogate `id`, PR #175). Defined once and reused by:
//!
//! - the `enrich_*` function signatures (`enrich_and_persist::*`),
//! - the SQS wire ([`super::EnrichmentMessage`]) the producer encodes and the
//!   worker decodes,
//! - the batch runner's candidate-row decode (`fetch_all::<AssetKey>()`).
//!
//! One `#[derive(Row)]` + serde struct serves all three — the same fields are
//! the ClickHouse RowBinary columns AND the JSON wire fields, so the (4-field)
//! asset key is not re-declared per crate.

use std::fmt;

use clickhouse::Row;
use serde::{Deserialize, Serialize};

/// `asset_enrichment` key = the `assets` ORDER BY tuple. Field order MUST match
/// `ORDER BY (asset_type, asset_code, issuer_id, contract_id)` for the
/// positional RowBinary decode in the candidate query.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Row)]
pub struct AssetKey {
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub contract_id: i64,
}

/// Compact `-`-joined label (`{asset_type}-{asset_code}-{issuer_id}-{contract_id}`)
/// — the canonical key form for logs / report samples. Defined once here so
/// callers use `key.to_string()` instead of re-spelling the field order.
impl fmt::Display for AssetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.asset_type, self.asset_code, self.issuer_id, self.contract_id
        )
    }
}

/// `nft_enrichment` key = the `nfts` ORDER BY tuple. `contract_id` is the
/// `soroban_contracts.id` FK (not the StrKey).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Row)]
pub struct NftKey {
    pub contract_id: i64,
    pub token_id: String,
}

/// Compact `-`-joined label (`{contract_id}-{token_id}`) — see [`AssetKey`]'s
/// `Display`.
impl fmt::Display for NftKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.contract_id, self.token_id)
    }
}
