//! Wire types for `GET /v1/search`.
//!
//! Spec source: lore task 0053 + canonical SQL in
//! `docs/architecture/database-schema/endpoint-queries/22_get_search.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Discriminated response: `redirect` for unambiguous exact match,
/// `results` for grouped broad search.
///
/// `#[serde(tag = "type")]` puts the discriminator on the wire as
/// `"type": "redirect" | "results"` per the task spec, mirroring the
/// frontend search-bar UX expectation: a `redirect` causes the bar to
/// navigate directly; a `results` shows the dropdown with grouped hits.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SearchResponse {
    Redirect(SearchRedirect),
    Results(SearchResults),
}

/// Redirect payload — frontend navigates directly to the entity page.

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchRedirect {
    pub entity_type: EntityType,
    pub entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<DateTime<Utc>>,
}

impl SearchRedirect {
    /// Synthesize a redirect from a singleton broad-search hit.
    ///
    /// Eligible entity types are the ones whose `(entity_type,
    /// entity_id)` pair is enough for the FE to route — transaction,
    /// account, contract, pool. Asset and NFT need additional routing
    /// fields (`surrogate_id`, `contract_id` + `token_id`) that the
    /// current `SearchRedirect` wire shape does not carry, so a
    /// singleton asset / NFT hit falls through to `Results` with one
    /// row instead.
    pub fn from_hit(hit: &SearchHit) -> Option<Self> {
        match hit.entity_type {
            EntityType::Transaction
            | EntityType::Account
            | EntityType::Contract
            | EntityType::Pool => Some(Self {
                entity_type: hit.entity_type,
                entity_id: hit.identifier.clone(),
                successful: hit.successful,
                last_activity_at: hit.last_activity_at,
            }),
            EntityType::Asset | EntityType::Nft => None,
        }
    }
}

/// Results payload — six entity-typed buckets, each capped at the
/// per-group `limit` chosen by the caller (default 10, ceiling 50).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SearchResults {
    pub groups: SearchGroups,
}

/// Per-entity buckets. Empty buckets are omitted from the JSON output
/// via `skip_serializing_if` — frontend treats absent and empty array
/// identically, and dropping empties keeps the dropdown payload tight
/// when a typed `?type=` filter narrows results to a single bucket.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SearchGroups {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transactions: Vec<SearchHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<SearchHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<SearchHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contracts: Vec<SearchHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nfts: Vec<SearchHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pools: Vec<SearchHit>,
}

/// Single search row. Same shape across every entity bucket.
///
/// `identifier` is the canonical human-shown id (hex hash for
/// transactions, strkey `L…` for pools, StrKey for accounts /
/// contracts, asset code for assets, name for NFTs). For `asset` it is
/// NOT unique — the frontend routes via `surrogate_id`. For `nft` it is
/// also not unique and `surrogate_id` alone is insufficient because NFT
/// identity is the composite `(contract_id, token_id)` per task 0264 /
/// ADR 0030 — the two fields below carry the composite for routing.
///
/// `successful` and `last_activity_at` are populated only for
/// `entity_type = transaction` today — joined from the partitioned
/// `transactions` table via `(hash, created_at)` for partition pruning.
/// Other entity types pass `None` for both until follow-up enrichment
/// adds per-entity last-activity joins. The frontend renders a
/// status chip + relative timestamp on the right side of the row
/// whenever these fields are present.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchHit {
    pub entity_type: EntityType,
    pub identifier: String,
    pub label: String,
    pub surrogate_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<DateTime<Utc>>,
    /// NFT composite routing key. Populated only when
    /// `entity_type = nft`; `None` for every other bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    /// NFT composite routing key. Populated only when
    /// `entity_type = nft`; `None` for every other bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
}

/// Entity discriminator. Closed allowlist used by the `type=` filter
/// and the `entity_type` field on every hit / redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Transaction,
    Account,
    Asset,
    Contract,
    Nft,
    Pool,
}

impl EntityType {
    /// Wire-format names for every variant. Single source of truth used
    /// by `parse()`, error messages, and any future API surface that
    /// needs to enumerate the closed set (e.g. an OpenAPI enum
    /// extension or a debug listing endpoint).
    pub const ALL: &'static [&'static str] =
        &["transaction", "contract", "asset", "account", "nft", "pool"];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "transaction" => Some(Self::Transaction),
            "account" => Some(Self::Account),
            "asset" => Some(Self::Asset),
            "contract" => Some(Self::Contract),
            "nft" => Some(Self::Nft),
            "pool" => Some(Self::Pool),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lock the invariant that `ALL` and `parse()` agree on the closed
    /// set. If a variant is added in the future and one site is missed,
    /// this catches the drift before the divergence reaches the wire.
    #[test]
    fn all_const_matches_parse_round_trip() {
        for name in EntityType::ALL {
            assert!(
                EntityType::parse(name).is_some(),
                "EntityType::ALL contains `{name}` but parse() rejects it"
            );
        }
    }

    /// NFT hit carries the composite `(contract_id, token_id)` on the
    /// wire so the frontend can route to `/nfts/:contract_id/:token_id`
    /// without a second roundtrip. The fields must serialize when set
    /// and disappear when `None` to keep the dropdown payload tight for
    /// non-NFT buckets.
    #[test]
    fn nft_hit_serializes_composite_fields() {
        let hit = SearchHit {
            entity_type: EntityType::Nft,
            identifier: "Cool Cat #7".to_string(),
            label: "CoolCats".to_string(),
            surrogate_id: Some(42),
            successful: None,
            last_activity_at: None,
            contract_id: Some(
                "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ".to_string(),
            ),
            token_id: Some("token-7".to_string()),
        };
        let json = serde_json::to_value(&hit).unwrap();
        assert_eq!(
            json["contract_id"],
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ"
        );
        assert_eq!(json["token_id"], "token-7");
    }

    /// Singleton-redirect synthesis is the load-bearing helper for the
    /// option C search refactor (task 0271). It must accept the four
    /// redirect-eligible entity types and reject asset / NFT (which
    /// need routing fields the wire does not carry today).
    #[test]
    fn from_hit_eligible_entity_types_produce_redirect() {
        for entity_type in [
            EntityType::Transaction,
            EntityType::Account,
            EntityType::Contract,
            EntityType::Pool,
        ] {
            let hit = SearchHit {
                entity_type,
                identifier: "ID".to_string(),
                label: String::new(),
                surrogate_id: None,
                successful: None,
                last_activity_at: None,
                contract_id: None,
                token_id: None,
            };
            let redirect = SearchRedirect::from_hit(&hit).expect("eligible");
            assert_eq!(redirect.entity_type, entity_type);
            assert_eq!(redirect.entity_id, "ID");
        }
    }

    #[test]
    fn from_hit_asset_and_nft_fall_through() {
        for entity_type in [EntityType::Asset, EntityType::Nft] {
            let hit = SearchHit {
                entity_type,
                identifier: "ID".to_string(),
                label: String::new(),
                surrogate_id: Some(1),
                successful: None,
                last_activity_at: None,
                contract_id: None,
                token_id: None,
            };
            assert!(
                SearchRedirect::from_hit(&hit).is_none(),
                "{entity_type:?} must fall through to Results"
            );
        }
    }

    #[test]
    fn from_hit_propagates_tx_enrichment_fields() {
        let hit = SearchHit {
            entity_type: EntityType::Transaction,
            identifier: "deadbeef".to_string(),
            label: String::new(),
            surrogate_id: None,
            successful: Some(true),
            last_activity_at: Some(
                "2026-01-01T00:00:00Z"
                    .parse::<DateTime<Utc>>()
                    .expect("ts parses"),
            ),
            contract_id: None,
            token_id: None,
        };
        let redirect = SearchRedirect::from_hit(&hit).expect("eligible");
        assert_eq!(redirect.successful, Some(true));
        assert!(redirect.last_activity_at.is_some());
    }

    #[test]
    fn non_nft_hit_omits_composite_fields() {
        let hit = SearchHit {
            entity_type: EntityType::Transaction,
            identifier: "deadbeef".to_string(),
            label: "ledger 1".to_string(),
            surrogate_id: None,
            successful: Some(true),
            last_activity_at: None,
            contract_id: None,
            token_id: None,
        };
        let json = serde_json::to_value(&hit).unwrap();
        assert!(json.get("contract_id").is_none());
        assert!(json.get("token_id").is_none());
    }
}
