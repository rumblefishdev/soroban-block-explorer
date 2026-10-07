//! Step 6 of state extraction: NFTs and their ownership events, from the
//! NFT events `nft::detect_nft_events` produced.

use super::*;

/// Detect NFTs from NFT events (produced by task 0026's `detect_nft_events`).
///
/// Converts `NftEvent` records into `ExtractedNft` entities for DB persistence.
pub fn detect_nfts(nft_events: &[NftEvent]) -> Vec<ExtractedNft> {
    let mut nfts = Vec::new();

    for event in nft_events {
        let token_id = token_id_to_string(&event.token_id);
        if token_id.is_empty() {
            continue;
        }

        let (owner, minted_at_ledger) = match event.event_kind.as_str() {
            "mint" => (event.to.clone(), Some(event.event_id.ledger_sequence)),
            "transfer" => (event.to.clone(), None),
            "burn" => (None, None),
            _ => continue,
        };

        nfts.push(ExtractedNft {
            contract_id: event.contract_id.clone(),
            token_id,
            collection_name: None,
            owner,
            name: None,
            media_url: None,
            minted_at_ledger,
            last_seen_ledger: event.event_id.ledger_sequence,
            created_at: event.created_at,
        });
    }

    nfts
}

/// Convert an NftEvent token_id JSON value to a string key for the DB.
fn token_id_to_string(token_id: &Value) -> String {
    if let Some(v) = token_id.get("value") {
        if v.is_null() {
            return String::new();
        }
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        if let Some(n) = v.as_i64() {
            return n.to_string();
        }
        if let Some(n) = v.as_u64() {
            return n.to_string();
        }
        return v.to_string();
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Step 6b: NFT Ownership Event Extraction (task 0202)
// ---------------------------------------------------------------------------

/// Transform raw parser `NftEvent` records into schema-shaped
/// `ExtractedNftEvent` rows for `nft_ownership_changes`.
///
/// The parser (`detect_nft_events`) emits events with a JSON-typed
/// `token_id`, string `event_kind` ("mint"/"transfer"/"burn"), and split
/// `from`/`to` fields. The persistence layer expects a stringified
/// `token_id`, the `NftEventType` enum, and a unified `owner`
/// field (`Some(to)` for mint/transfer, `None` for burn).
///
/// Events with empty `token_id` are skipped (matches `detect_nfts`
/// behaviour). Events with `event_kind` not in {"mint","transfer","burn"}
/// are skipped — the parser already restricts emission to these three
/// kinds, so the guard is defensive.
///
/// Every other event is kept: a row's place is its stellar-rpc `event_id`
/// (task 0424), so however many changes one token sees in one ledger, none
/// is dropped.
#[instrument(skip(events), fields(event_count = events.len()))]
pub fn extract_nft_ownership_events(events: &[NftEvent]) -> Vec<ExtractedNftEvent> {
    let mut out: Vec<ExtractedNftEvent> = Vec::with_capacity(events.len());

    for event in events {
        let token_id = token_id_to_string(&event.token_id);
        if token_id.is_empty() {
            continue;
        }

        let event_type = match event.event_kind.parse::<NftEventType>() {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    event_kind = %event.event_kind,
                    error = %e,
                    "unknown NFT event_kind — skipping (parser should not emit this)"
                );
                continue;
            }
        };

        let owner = match event_type {
            NftEventType::Mint | NftEventType::Transfer => event.to.clone(),
            NftEventType::Burn => None,
        };

        out.push(ExtractedNftEvent {
            transaction_hash: event.transaction_hash.clone(),
            contract_id: event.contract_id.clone(),
            token_id,
            event_type,
            owner,
            created_at: event.created_at,
            event_id: event.event_id,
        });
    }

    out
}
