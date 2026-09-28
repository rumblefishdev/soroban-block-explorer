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

        let (owner_account, minted_at_ledger) = match event.event_kind.as_str() {
            "mint" => (event.to.clone(), Some(event.ledger_sequence)),
            "transfer" => (event.to.clone(), None),
            "burn" => (None, None),
            _ => continue,
        };

        nfts.push(ExtractedNft {
            contract_id: event.contract_id.clone(),
            token_id,
            collection_name: None,
            owner_account,
            name: None,
            media_url: None,
            minted_at_ledger,
            last_seen_ledger: event.ledger_sequence,
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
/// `ExtractedNftEvent` rows for `nft_ownership`.
///
/// The parser (`detect_nft_events`) emits events with a JSON-typed
/// `token_id`, string `event_kind` ("mint"/"transfer"/"burn"), and split
/// `from`/`to` fields. The persistence layer expects a stringified
/// `token_id`, the `NftEventType` enum, and a unified `owner_account`
/// field (`Some(to)` for mint/transfer, `None` for burn).
///
/// Additionally, this fn computes `event_order` — a per-`(contract, token,
/// ledger)` monotonic ordinal (SMALLINT) required by the schema PK
/// `(nft_id, created_at, ledger_sequence, event_order)` and by the
/// LEAD-window pagination in `17_get_nfts_transfers.sql`.
///
/// Events with empty `token_id` are skipped (matches `detect_nfts`
/// behaviour). Events with `event_kind` not in {"mint","transfer","burn"}
/// are skipped — the parser already restricts emission to these three
/// kinds, so the guard is defensive.
///
/// Pathological-input guard: `event_order` is persisted as SMALLINT so
/// the schema bound is `i16::MAX = 32_767`. Once a single
/// `(contract, token, ledger)` triple has already produced that many
/// rows, further events for the same triple are skipped with a warn
/// instead of overflowing the staging `try_into::<i16>()` and failing
/// the whole ledger. No real NFT contract reaches this bound; the cap
/// exists to keep ingestion robust against a malicious / buggy
/// contract emitting tens of thousands of events for one NFT in a
/// single ledger.
#[instrument(skip(events), fields(event_count = events.len()))]
pub fn extract_nft_ownership_events(events: &[NftEvent]) -> Vec<ExtractedNftEvent> {
    /// SMALLINT max — `nft_ownership.event_order` is stored as i16 in PG.
    const MAX_EVENT_ORDER: u16 = i16::MAX as u16;

    let mut order_counter: HashMap<(String, String, u32), u16> = HashMap::new();
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

        let owner_account = match event_type {
            NftEventType::Mint | NftEventType::Transfer => event.to.clone(),
            NftEventType::Burn => None,
        };

        let key = (
            event.contract_id.clone(),
            token_id.clone(),
            event.ledger_sequence,
        );
        let counter = order_counter.entry(key).or_insert(0);
        if *counter > MAX_EVENT_ORDER {
            warn!(
                contract_id = %event.contract_id,
                token_id = %token_id,
                ledger_sequence = event.ledger_sequence,
                max = MAX_EVENT_ORDER,
                "event_order would exceed SMALLINT range; skipping further events for triple"
            );
            continue;
        }
        let event_order = *counter;
        *counter = counter.saturating_add(1);

        out.push(ExtractedNftEvent {
            transaction_hash: event.transaction_hash.clone(),
            contract_id: event.contract_id.clone(),
            token_id,
            event_type,
            owner_account,
            event_order,
            ledger_sequence: event.ledger_sequence,
            created_at: event.created_at,
        });
    }

    out
}
