//! NFT event detection from extracted Soroban events.
//!
//! Identifies mint, transfer, and burn events that match NFT patterns
//! (SEP-0050 and known non-standard conventions). Produces `NftEvent`
//! structs for consumption by task 0027 (NFT state derivation).
//!
//! Detection is based on event topic patterns. The first topic is expected
//! to be a Symbol naming the event kind. Case-insensitive matching handles
//! both standard ("transfer") and non-standard ("Transfer") conventions.

use serde_json::Value;
use tracing::warn;

use crate::sac::{SacOverride, sac_override_from_event_topics, topic_symbol_value};
use crate::scval::map_get;
use crate::types::{ExtractedEvent, NftEvent};
use domain::ContractEventType;

/// Detect NFT-related events from a list of extracted events.
///
/// Scans for topic patterns matching NFT mint/transfer/burn events.
/// Only events with a non-null `contract_id` are considered.
///
/// Returns detected `NftEvent` items. Events that don't match any
/// NFT pattern are silently skipped.
///
/// `net_id` (`network_id(passphrase)`) gates out classic-asset SAC events at
/// their source (task 0294): a CAP-67 SAC `transfer`/`mint`/`burn` carries the
/// SEP-11 asset string in its LAST topic and an i128 AMOUNT in `data`, so when
/// `derive_sac(asset) == emitter` it is provably a fungible amount and can never
/// be an NFT. Dropping it here stops the amount being mis-read as a token_id and
/// minted as a false NFT candidate. The gate is false-negative-only — a bespoke
/// contract (`derived != emitter`) or non-SAC signature returns `None`, so real
/// NFTs are unaffected.
pub fn detect_nft_events(events: &[ExtractedEvent], net_id: &[u8; 32]) -> Vec<NftEvent> {
    let mut nft_events = Vec::new();

    for event in events {
        if event.event_type != ContractEventType::Contract {
            continue;
        }
        let Some(ref contract_id) = event.contract_id else {
            continue;
        };

        let topics = match event.topics.as_array() {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        // Task 0294 — drop crypto-proven classic-asset SAC events before they
        // can be minted as false NFT candidates. `derive_sac(asset) == emitter`
        // is cryptographic proof the i128 in `data` is a transfer AMOUNT, not an
        // NFT token_id. `None` for bespoke contracts / non-SAC signatures, so
        // real NFTs pass through (false-negative-only).
        if sac_override_from_event_topics(contract_id, &event.topics, net_id).is_some() {
            continue;
        }

        let first_topic = topic_symbol_value(&topics[0]);
        let first_lower = first_topic.to_ascii_lowercase();

        // consecutive_mint (OZ Consecutive / EIP-2309) expands a [from,to] range
        // to N mints, so it yields multiple events — handle it before the
        // single-event symbols below.
        if first_lower == "consecutive_mint" {
            match try_parse_consecutive_mint(contract_id, &topics[1..], &event.data, event) {
                Some(mut evs) => nft_events.append(&mut evs),
                None => maybe_tripwire(contract_id, &first_lower, event, topics.len()),
            }
            continue;
        }

        let parsed = match first_lower.as_str() {
            "transfer" => try_parse_transfer(contract_id, &topics[1..], &event.data, event),
            "mint" => try_parse_mint(contract_id, &topics[1..], &event.data, event),
            "burn" => try_parse_burn(contract_id, &topics[1..], &event.data, event),
            // Not an NFT-candidate symbol — skip silently (no tripwire).
            _ => continue,
        };

        match parsed {
            Some(nft) => nft_events.push(nft),
            // Candidate symbol but no known arg shape (Shape A topics+scalar,
            // Shape B packed-vec, Shape C topics+map{token_id}) parsed — drop and
            // tripwire so genuinely-unhandled NFT shapes surface. Fungible maps
            // are suppressed inside maybe_tripwire.
            None => maybe_tripwire(contract_id, &first_lower, event, topics.len()),
        }
    }

    nft_events
}

/// Crypto-proven un-deployed-SAC overrides for the ledger's events (task 0323).
///
/// An un-deployed SAC emits a CAP-67 unified event under its reserved C… address
/// without ever being deployed. [`sac_override_from_event_topics`] proves
/// `emitter == derive_sac(asset)`. We collect those overrides (deduped by contract
/// id) so staging models them as ASSETS, not `soroban_contracts` rows: the override
/// id suppresses the Pass-2 FK stub, and its `identity` seeds the `assets` row.
/// Replaces the trustline-sourced `derive_sac_overrides_from_assets` — an
/// un-deployed SAC matters once it has activity (an event), not at the trustline.
//
// ponytail: re-runs the SAC gate already evaluated in `detect_nft_events`; the
// expensive SHA256 only fires for genuine SAC-shaped events (last topic parses as
// a SEP-11 asset), so the double-cost is bounded. Fold the collect into
// `detect_nft_events` if profiling shows it hot.
pub fn detect_undeployed_sac_overrides(
    events: &[(String, Vec<ExtractedEvent>)],
    net_id: &[u8; 32],
) -> Vec<SacOverride> {
    let mut by_cid: std::collections::HashMap<String, SacOverride> =
        std::collections::HashMap::new();
    for (_tx_hash, evs) in events {
        for ev in evs {
            let Some(cid) = &ev.contract_id else {
                continue;
            };
            if by_cid.contains_key(cid) {
                continue;
            }
            if let Some(ov) = sac_override_from_event_topics(cid, &ev.topics, net_id) {
                by_cid.insert(cid.clone(), ov);
            }
        }
    }
    by_cid.into_values().collect()
}

/// Try to parse a transfer event as an NFT transfer.
///
/// SEP-0050 pattern: topics = [Symbol("transfer"), Address(from), Address(to)], data = token_id
/// We only emit if the data looks like a token ID (not an i128 amount).
fn try_parse_transfer(
    contract_id: &str,
    remaining_topics: &[Value],
    data: &Value,
    event: &ExtractedEvent,
) -> Option<NftEvent> {
    let (addrs, token_id) = extract_args(remaining_topics, data, 2)?;
    Some(NftEvent {
        transaction_hash: event.transaction_hash.clone(),
        contract_id: contract_id.to_string(),
        event_kind: "transfer".into(),
        token_id,
        from: Some(addrs[0].clone()),
        to: Some(addrs[1].clone()),
        ledger_sequence: event.event_id.ledger_sequence,
        created_at: event.created_at,
    })
}

/// Try to parse a mint event as an NFT mint.
///
/// SEP-0050 pattern: topics = [Symbol("mint"), Address(to)], data = token_id
fn try_parse_mint(
    contract_id: &str,
    remaining_topics: &[Value],
    data: &Value,
    event: &ExtractedEvent,
) -> Option<NftEvent> {
    let (addrs, token_id) = extract_args(remaining_topics, data, 1)?;
    Some(NftEvent {
        transaction_hash: event.transaction_hash.clone(),
        contract_id: contract_id.to_string(),
        event_kind: "mint".into(),
        token_id,
        from: None,
        to: Some(addrs[0].clone()),
        ledger_sequence: event.event_id.ledger_sequence,
        created_at: event.created_at,
    })
}

/// Try to parse a burn event as an NFT burn.
///
/// Burn is not standardized in SEP-0050 core but some contracts emit it.
/// Pattern: topics = [Symbol("burn"), Address(from)], data = token_id
fn try_parse_burn(
    contract_id: &str,
    remaining_topics: &[Value],
    data: &Value,
    event: &ExtractedEvent,
) -> Option<NftEvent> {
    let (addrs, token_id) = extract_args(remaining_topics, data, 1)?;
    Some(NftEvent {
        transaction_hash: event.transaction_hash.clone(),
        contract_id: contract_id.to_string(),
        event_kind: "burn".into(),
        token_id,
        from: Some(addrs[0].clone()),
        to: None,
        ledger_sequence: event.event_id.ledger_sequence,
        created_at: event.created_at,
    })
}

/// Maximum tokens a single `consecutive_mint` may expand to — guards against a
/// malformed/hostile range exploding memory/CPU in the (Lambda) indexer. Real
/// ranges are tiny (≤146 observed); over-cap ranges are dropped + tripwired, not
/// expanded. Sized to the downstream i16 event-order cap.
const MAX_CONSECUTIVE_RANGE: u64 = 65_535;

/// Parse an OZ Consecutive `consecutive_mint` (EIP-2309 analog) into one mint per
/// token_id in the inclusive `[from_token_id, to_token_id]` range.
///
/// Topics = [Symbol("consecutive_mint"), Address(to)]; data is either
/// `map{from_token_id, to_token_id}` or `vec[from, to]` — both observed on
/// mainnet (2026-06-17) — each an unsigned int. Returns `None` (→ tripwire) for
/// an unrecognised layout, an inverted range, or an implausibly large range.
fn try_parse_consecutive_mint(
    contract_id: &str,
    remaining_topics: &[Value],
    data: &Value,
    event: &ExtractedEvent,
) -> Option<Vec<NftEvent>> {
    let to = remaining_topics.first().and_then(topic_address_value)?;

    let (from_v, to_v) = if let Some(elems) = data_vec_elements(data) {
        (elems.first()?, elems.get(1)?)
    } else {
        (
            map_get(data, "from_token_id")?,
            map_get(data, "to_token_id")?,
        )
    };
    let (from_id, to_id) = consecutive_range(from_v, to_v)?;
    if to_id < from_id || to_id - from_id >= MAX_CONSECUTIVE_RANGE {
        return None;
    }

    Some(
        (from_id..=to_id)
            .map(|id| NftEvent {
                transaction_hash: event.transaction_hash.clone(),
                contract_id: contract_id.to_string(),
                event_kind: "mint".into(),
                token_id: serde_json::json!({ "type": "u64", "value": id }),
                from: None,
                to: Some(to.clone()),
                ledger_sequence: event.event_id.ledger_sequence,
                created_at: event.created_at,
            })
            .collect(),
    )
}

/// Extract the `(from, to)` token-id bounds from a `consecutive_mint`'s two range
/// operands, accepting only unsigned-int ScVals (u32/u64; wider ids tripwire).
fn consecutive_range(from_v: &Value, to_v: &Value) -> Option<(u64, u64)> {
    let from_ty = from_v.get("type").and_then(|v| v.as_str())?;
    let to_ty = to_v.get("type").and_then(|v| v.as_str())?;
    if !matches!(from_ty, "u32" | "u64") || !matches!(to_ty, "u32" | "u64") {
        return None;
    }
    let from = from_v.get("value").and_then(|v| v.as_u64())?;
    let to = to_v.get("value").and_then(|v| v.as_u64())?;
    Some((from, to))
}

/// Emit the NFT tripwire unless the unparsed payload is a recognisably-fungible
/// map (CAP-67/SAC `map{amount,to_muxed_id}`) — those are not NFTs and would
/// bury the real signal under ~148M events.
fn maybe_tripwire(contract_id: &str, event_kind: &str, event: &ExtractedEvent, topic_count: usize) {
    if is_fungible_map(&event.data) {
        return;
    }
    warn!(
        target: "xdr_parser::nft",
        contract_id = %contract_id,
        event_kind,
        data_type = event.data.get("type").and_then(|v| v.as_str()).unwrap_or("?"),
        topic_count,
        "NFT event symbol matched but no known arg shape parsed — row dropped. \
         Extend nft.rs shapes if this is a real NFT."
    );
}

/// Check if data looks like a token ID (scalar value, not a complex structure).
///
/// Both SEP-0041 (fungible) and SEP-0050 (NFT) use the same `["transfer",
/// Address(from), Address(to)]` topic pattern for transfer/mint/burn events
/// — only the data payload differentiates them. The payload type alone
/// **cannot** reliably tell NFT from fungible: real mainnet NFTs (e.g. the
/// James Bachini SEP-39 collection `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`
/// — confirmed live 2026-05-13 via `stellar contract fetch`) use `i128`
/// for `token_id` (SEP-39 / ERC-721 style), and SEP-41 fungible transfers
/// use `i128` for `amount`. **Both shapes are observed on mainnet.**
///
/// Task 0118 originally tried a Patch C whitelist (rejecting `i128`/`u128`
/// at the parser) but that approach silently drops legitimate SEP-39 NFTs
/// — the 2026-05-12 CH pilot audit sample didn't contain one so the
/// false-negative wasn't visible in measurement; the 2026-05-13 pre-audit
/// re-test against live mainnet RPC found one. **Patch C was reverted.**
///
/// Authoritative NFT-vs-fungible discrimination lives downstream, at
/// persist time, via the WASM-spec-based classifier
/// (`xdr_parser::classify_contract_from_wasm_spec` + the persist routing
/// in `crates/indexer/src/handler/persist/write.rs::resolve_nft_filter`):
///
/// - `Fungible` / `Token` (SAC) verdict → row dropped before INSERT.
/// - `Nft` verdict → row to hot `nfts` table.
/// - `Other` / NULL verdict → row to `nfts_pending` quarantine
///   (task 0217), promoted on later WASM observation.
///
/// This function therefore only rejects clearly non-token shapes (void,
/// maps, vecs, errors). Everything else is forwarded for the classifier
/// to judge.
fn looks_like_token_id(data: &Value) -> bool {
    let type_str = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
    !matches!(type_str, "void" | "map" | "vec" | "error")
}

/// True for a data map recognisable as a fungible/SAC event payload: no
/// `token_id` key, but carrying `amount` or `to_muxed_id` (the CAP-67 muxed
/// transfer/mint shape — ~148M on mainnet vs ~890 NFT `map{token_id}` mints).
/// Used to suppress the NFT tripwire for these known non-NFT maps.
fn is_fungible_map(data: &Value) -> bool {
    map_get(data, "token_id").is_none()
        && (map_get(data, "amount").is_some() || map_get(data, "to_muxed_id").is_some())
}

/// Return the elements of a `Vec`-typed ScVal JSON payload, iff `data` is a
/// `{"type":"vec","value":[…]}`. Used for the "packed" NFT event shape where
/// the event args ride in the data tuple instead of the topics.
fn data_vec_elements(data: &Value) -> Option<&[Value]> {
    if data.get("type").and_then(|v| v.as_str()) != Some("vec") {
        return None;
    }
    data.get("value")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
}

/// Collect every element as an address string, or `None` if any element is
/// not a valid address.
fn collect_addresses(items: &[Value]) -> Option<Vec<String>> {
    items.iter().map(topic_address_value).collect()
}

/// Extract `n_addrs` address strings plus a `token_id` value from an NFT
/// event, tolerating the layouts observed on mainnet:
///
/// - **Shape A** (SEP-50 NFT / SEP-41 standard): addresses in topics, token_id
///   in data.
/// - **Shape B** (packed, non-standard): the whole `(addr…, token_id)` tuple in
///   a data `Vec` — e.g. Bachini / ERC-721-port contracts (verified on-chain).
///
/// Returns `None` for map-shaped or otherwise unrecognised payloads — the
/// caller surfaces those via the `detect_nft_events` tripwire rather than
/// silently dropping them. (A token_id-as-extra-topic layout was considered
/// but cut: no SEP defines it and no on-chain instance was found; the tripwire
/// will surface it if it ever appears.)
fn extract_args(
    remaining_topics: &[Value],
    data: &Value,
    n_addrs: usize,
) -> Option<(Vec<String>, Value)> {
    // Shape A: addresses in topics, token_id scalar in data.
    if remaining_topics.len() >= n_addrs
        && looks_like_token_id(data)
        && let Some(addrs) = collect_addresses(&remaining_topics[..n_addrs])
    {
        return Some((addrs, data.clone()));
    }

    // Shape B: (addresses…, token_id) packed in a data Vec.
    if let Some(elems) = data_vec_elements(data)
        && elems.len() == n_addrs + 1
    {
        let token_id = &elems[n_addrs];
        if looks_like_token_id(token_id)
            && let Some(addrs) = collect_addresses(&elems[..n_addrs])
        {
            return Some((addrs, token_id.clone()));
        }
    }

    // Shape C (canonical OZ / SEP-50): addresses in topics, token_id in a data
    // map under the "token_id" key — the soroban-sdk `#[contractevent]`
    // map-by-field-name default, the dominant modern NFT shape (confirmed live
    // on mainnet 2026-06-17: CCHHGIOB… token_id 93, CARTUL5A… token_id 133). A
    // map without a token_id key falls through to the caller, which skips
    // known-fungible maps and tripwires the rest.
    if remaining_topics.len() >= n_addrs
        && let Some(token_id) = map_get(data, "token_id")
        && looks_like_token_id(token_id)
        && let Some(addrs) = collect_addresses(&remaining_topics[..n_addrs])
    {
        return Some((addrs, token_id.clone()));
    }

    None
}

/// Extract an address string from a tagged ScVal JSON topic.
///
/// Only accepts topics typed as "address" with a non-empty string value.
/// Returns `None` for non-address topics so callers can skip events
/// with invalid address fields.
fn topic_address_value(topic: &Value) -> Option<String> {
    let type_str = topic.get("type").and_then(|v| v.as_str());
    if type_str == Some("address")
        && let Some(s) = topic.get("value").and_then(|v| v.as_str())
        && !s.is_empty()
    {
        return Some(s.to_string());
    }
    None
}

#[cfg(test)]
mod tests;
