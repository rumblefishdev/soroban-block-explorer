//! Wire contract for the enrichment SQS queue — the single source of
//! truth shared by the producer (indexer `enrichment_publish`, which
//! encodes it) and the `enrichment-worker` Lambda (which decodes it).
//!
//! Keyed by the ClickHouse composite identity. CH dropped the Postgres
//! surrogate `assets.id` / `nfts.id` (PR #175), so the wire carries the
//! ORDER BY tuple of the target table instead of a single integer.

use serde::{Deserialize, Serialize};

use super::key::{AssetKey, NftKey};

/// One enrichment unit of work. `kind` tags the variant on the wire
/// (`{"kind":"sep1_assets",…}` / `{"kind":"nft_token_uri",…}`). Encoded by
/// the producer, decoded by the worker — keep both sides on this type so
/// the format cannot drift.
///
/// The variant payloads are the shared [`AssetKey`] / [`NftKey`]; the internal
/// `kind` tag inlines their fields, so the wire stays flat
/// (`{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC",…}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnrichmentMessage {
    /// SEP-1 issuer-TOML enrichment for a classic/SAC asset.
    Sep1Assets(AssetKey),
    /// NFT `token_uri()` metadata enrichment.
    NftTokenUri(NftKey),
}

#[cfg(test)]
mod tests {
    use super::EnrichmentMessage;
    use crate::enrich_and_persist::{AssetKey, NftKey};

    /// Pin the exact on-wire bytes both sides depend on: the producer
    /// (`indexer::enrichment_publish`) encodes this type, the worker decodes it.
    /// A serde change (field rename, tag) that would silently break the queue
    /// fails here instead.
    #[test]
    fn sep1_wire_format_is_flat_and_stable() {
        let msg = EnrichmentMessage::Sep1Assets(AssetKey {
            asset_type: 1,
            asset_code: "USDC".into(),
            issuer_id: 42,
            contract_id: 0,
        });
        let json = serde_json::to_string(&msg).expect("encode");
        assert_eq!(
            json,
            r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":42,"contract_id":0}"#
        );
        assert_eq!(
            serde_json::from_str::<EnrichmentMessage>(&json).expect("decode"),
            msg
        );
    }

    #[test]
    fn nft_wire_format_is_flat_and_stable() {
        let msg = EnrichmentMessage::NftTokenUri(NftKey {
            contract_id: 7,
            token_id: "1".into(),
        });
        let json = serde_json::to_string(&msg).expect("encode");
        assert_eq!(
            json,
            r#"{"kind":"nft_token_uri","contract_id":7,"token_id":"1"}"#
        );
        assert_eq!(
            serde_json::from_str::<EnrichmentMessage>(&json).expect("decode"),
            msg
        );
    }
}
