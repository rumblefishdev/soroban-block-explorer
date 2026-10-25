//! SAC contract_id derivation from `ContractIdPreimage`.
//!
//! Per stellar-core, the hash input is the XDR encoding of the full
//! `HashIdPreimage::ContractId` envelope (tag + network_id + preimage),
//! not the bare preimage:
//!
//! ```text
//! network_id  = SHA256(network_passphrase)
//! contract_id = SHA256(XDR.serialize(HashIdPreimage::ContractId {
//!                  network_id,
//!                  contract_id_preimage,
//!              }))
//! ```
//!
//! The 32-byte hash is rendered as a `C...` StrKey via `ScAddress`.
//!
//! Canonical passphrases come from the Stellar protocol documentation:
//! <https://developers.stellar.org/docs/data/rpc/api-reference/methods/getNetwork>.

use core::str::FromStr;

use sha2::{Digest, Sha256};
use stellar_xdr::{
    AccountId, AlphaNum4, AlphaNum12, Asset, AssetCode4, AssetCode12, ContractId,
    ContractIdPreimage, Hash, HashIdPreimage, HashIdPreimageContractId, Limits, ScAddress,
    WriteXdr,
};

use crate::error::{ParseError, ParseErrorKind};
use crate::types::SacAssetIdentity;

pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
pub const FUTURENET_PASSPHRASE: &str = "Test SDF Future Network ; October 2022";

/// Map a logical network name to its canonical passphrase. Case-insensitive.
/// Returns `None` for unknown names so the caller can fail explicitly rather
/// than silently defaulting.
pub fn passphrase_for(network: &str) -> Option<&'static str> {
    match network.to_ascii_lowercase().as_str() {
        "mainnet" | "public" | "pubnet" => Some(MAINNET_PASSPHRASE),
        "testnet" => Some(TESTNET_PASSPHRASE),
        "futurenet" => Some(FUTURENET_PASSPHRASE),
        _ => None,
    }
}

/// `network_id = SHA256(passphrase_bytes)`.
pub fn network_id(passphrase: &str) -> [u8; 32] {
    Sha256::digest(passphrase.as_bytes()).into()
}

/// Process-global network id, derived once from `STELLAR_NETWORK_PASSPHRASE`.
/// The single cache the indexer's cold-start primes and reads: `init_network_id`
/// validates the env and triggers this; the indexer's `network_id()` reads it.
/// `None` when the env is unset (legacy / dev tools / unit tests). Callers that
/// must pin a specific id pass it explicitly — e.g. `network_id(MAINNET_PASSPHRASE)`
/// — rather than relying on this env-derived accessor. (Audit A4 consolidated a
/// duplicate `OnceLock` that had lived in the indexer into this one.)
pub fn net_id() -> Option<&'static [u8; 32]> {
    static NET_ID: std::sync::OnceLock<Option<[u8; 32]>> = std::sync::OnceLock::new();
    NET_ID
        .get_or_init(|| {
            let raw = std::env::var("STELLAR_NETWORK_PASSPHRASE").ok()?;
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| network_id(trimmed))
        })
        .as_ref()
}

/// Derive the SAC `contract_id` StrKey from a `ContractIdPreimage` and the
/// network identifier, matching stellar-core's derivation.
///
/// The hash input is the XDR encoding of the full
/// `HashIdPreimage::ContractId` envelope (tag + network_id + preimage),
/// not the raw preimage alone — stellar-core wraps it that way so the
/// envelope-type discriminator is part of the hash input.
///
/// Returns the 56-char `C...` StrKey.
pub fn derive_sac_contract_id(
    preimage: &ContractIdPreimage,
    network_id: &[u8; 32],
) -> Result<String, ParseError> {
    let envelope = HashIdPreimage::ContractId(HashIdPreimageContractId {
        network_id: Hash(*network_id),
        contract_id_preimage: preimage.clone(),
    });
    let xdr_bytes = envelope.to_xdr(Limits::none()).map_err(|e| ParseError {
        kind: ParseErrorKind::XdrSerializationFailed,
        message: format!("serialize HashIdPreimage::ContractId: {e}"),
        context: None,
    })?;

    let digest: [u8; 32] = Sha256::digest(&xdr_bytes).into();
    Ok(ScAddress::Contract(ContractId(Hash(digest))).to_string())
}

/// Re-derive a classic / native asset's SAC `C…` StrKey from its identity
/// (ADR 0051). The StrKey is **not stored** — the API derives it on read for
/// display, since it is a pure function of `code:issuer`. Empty `asset_code`
/// AND `issuer` ⇒ the native (XLM) SAC. Returns `None` for a malformed issuer
/// StrKey or an over-long code (caller renders no contract link).
pub fn derive_sac_strkey(asset_code: &str, issuer: &str, network_id: &[u8; 32]) -> Option<String> {
    let asset = if asset_code.is_empty() && issuer.is_empty() {
        Asset::Native
    } else {
        let issuer_acct = AccountId::from_str(issuer).ok()?;
        build_credit_asset(asset_code, issuer_acct).ok()?
    };
    let preimage = ContractIdPreimage::Asset(asset);
    derive_sac_contract_id(&preimage, network_id).ok()
}

/// Crypto-proven un-deployed-SAC `(contract_id, identity)` pair (task 0323).
///
/// Produced by `detect_undeployed_sac_overrides` for every event-emitting
/// un-deployed SAC in a ledger (the `sac_override_from_event_topics` gate).
/// On the CH path each (a) suppresses the Pass-2 FK stub (no contract row) and
/// (b) seeds a SAC `assets` row from its identity. (The legacy PG persist path
/// still consumes them to flip `is_sac=true` on pre-window SAC skeletons.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SacOverride {
    /// Derived SAC `C…` StrKey for the asset.
    pub contract_id: String,
    /// Asset identity carried into `assets.asset_code` / `.issuer_id`
    /// for the corresponding `assets` row.
    pub identity: SacAssetIdentity,
}

/// Crypto-match-gated SAC override for a single event's `(emitter, topics)`.
///
/// Returns `Some` only when `emitter` IS the SAC contract for the asset carried
/// in the event's LAST topic (`emitter == derive_sac(asset)`) and the event's
/// first topic is a SAC-control signature. Shared gate (task 0294) for the live
/// NFT-detection path ([`crate::detect_nft_events`], which skips a crypto-proven
/// SAC event before it can be minted as a false NFT candidate) and the batch
/// orphan-relabel pass — both apply the identical gate so a bespoke WASM emitter
/// of a SAC-shaped event is never mislabeled `is_sac`.
///
/// `net_id` is `network_id(passphrase)` — hoist it out of any loop.
pub fn sac_override_from_event_topics(
    emitter: &str,
    topics: &serde_json::Value,
    net_id: &[u8; 32],
) -> Option<SacOverride> {
    let topics = topics.as_array()?;
    // Signature topic + the trailing asset topic at minimum.
    if topics.len() < 2 {
        return None;
    }
    let signature = topic_symbol_value(&topics[0]);
    if !SAC_CONTROL_EVENT_SIGNATURES
        .iter()
        .any(|s| signature.eq_ignore_ascii_case(s))
    {
        return None;
    }
    // The SEP-11 asset string rides in the LAST topic across every SAC event
    // shape — CAP-67 §"Unified Asset Events" defines transfer/mint/burn/clawback/
    // set_authorized all as `[sym, …, sep0011_asset]` (asset last; the i128 is in
    // `data`, never a topic). This is the load-bearing invariant: the gate keys
    // off the last topic being the asset string, so a (hypothetical) SAC event
    // that ever omitted the asset topic would NOT be gated and could re-open the
    // false-NFT path. Spec-verified + mainnet-confirmed (every sampled SAC event
    // carries it); a non-SAC contract's last topic is an address/scalar, so it
    // returns None below and is unaffected.
    let asset_str = topics.last().and_then(topic_string_value)?;
    let asset = parse_sac_asset_string(&asset_str)?;
    let preimage = ContractIdPreimage::Asset(asset.clone());
    let derived = derive_sac_contract_id(&preimage, net_id).ok()?;
    // Crypto-match gate (task 0294 C1): a contract is the asset's SAC only if
    // its own id IS the derived SAC id.
    if derived != emitter {
        return None;
    }
    Some(SacOverride {
        contract_id: derived,
        identity: asset_to_identity(&asset),
    })
}

/// The asset a deployed SAC wraps, read from the SAC's own instance entry
/// (task 0571).
///
/// From creation on, a SAC keeps `METADATA.name` — the SEP-11 string of its
/// asset — in instance storage, whoever deployed it. The envelope names the
/// asset only when the deploy is a top-level `CreateContract` or an auth
/// entry; another contract deploying the SAC leaves it out, so the instance
/// is the one source that holds for every deployment. The name is accepted
/// only when it derives to `contract_id`, the same proof the event path asks.
///
/// `instance` is the instance entry's typed `val`.
pub fn sac_identity_from_instance(
    contract_id: &str,
    instance: &serde_json::Value,
    network_id: &[u8; 32],
) -> Option<SacAssetIdentity> {
    let storage = crate::scval::typed(instance, "contract_instance")?
        .get("storage")?
        .as_array()?;
    let metadata = storage
        .iter()
        .find(|e| e.get("key").and_then(crate::scval::symbol) == Some("METADATA"))?;
    let name = crate::scval::typed_str(
        crate::scval::map_get(metadata.get("value")?, "name")?,
        "string",
    )?;
    let asset = parse_sac_asset_string(name)?;
    let derived =
        derive_sac_contract_id(&ContractIdPreimage::Asset(asset.clone()), network_id).ok()?;
    if derived != contract_id {
        return None;
    }
    Some(asset_to_identity(&asset))
}

/// Classic-asset SAC control-event signatures. `transfer`/`mint`/`burn` are
/// shared with bespoke tokens, but `clawback`/`set_authorized` are SAC-only
/// (a custom Soroban NFT never emits them). The crypto-match gate in
/// [`sac_override_from_event_topics`] is what makes the shared signatures
/// safe to act on.
const SAC_CONTROL_EVENT_SIGNATURES: &[&str] =
    &["transfer", "mint", "burn", "clawback", "set_authorized"];

/// Extract an `ScVal::Symbol` string from a tagged JSON topic (`type: "sym"`).
/// Shared with NFT detection (`crate::nft`) — one canonical copy.
pub(crate) fn topic_symbol_value(topic: &serde_json::Value) -> String {
    if topic.get("type").and_then(|v| v.as_str()) == Some("sym")
        && let Some(s) = topic.get("value").and_then(|v| v.as_str())
    {
        return s.to_string();
    }
    String::new()
}

/// Extract an `ScVal::String` value from a tagged JSON topic (`type: "string"`).
fn topic_string_value(topic: &serde_json::Value) -> Option<String> {
    if topic.get("type").and_then(|v| v.as_str()) == Some("string")
        && let Some(s) = topic.get("value").and_then(|v| v.as_str())
        && !s.is_empty()
    {
        return Some(s.to_string());
    }
    None
}

/// Parse a SEP-11 asset string (`"native"` or `"CODE:ISSUER"`) into an XDR
/// `Asset`. Returns `None` for malformed strings or invalid issuer StrKeys.
fn parse_sac_asset_string(s: &str) -> Option<Asset> {
    if s == "native" {
        return Some(Asset::Native);
    }
    let (code, issuer) = s.split_once(':')?;
    if code.is_empty() {
        return None;
    }
    let issuer_acct = AccountId::from_str(issuer).ok()?;
    build_credit_asset(code, issuer_acct).ok()
}

/// Build the XDR `Asset` for a classic credit asset given `(code, issuer)`.
///
/// Picks `CreditAlphanum4` for codes ≤4 bytes and `CreditAlphanum12`
/// for 5–12 bytes. Returns `Err(())` if the code length exceeds 12
/// (caller logs + skips). Trailing-NUL padding mirrors the on-chain
/// representation: stellar-core pads codes shorter than the slot length.
fn build_credit_asset(code: &str, issuer: AccountId) -> Result<Asset, ()> {
    let bytes = code.as_bytes();
    if bytes.len() <= 4 {
        let mut padded = [0u8; 4];
        padded[..bytes.len()].copy_from_slice(bytes);
        Ok(Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(padded),
            issuer,
        }))
    } else if bytes.len() <= 12 {
        let mut padded = [0u8; 12];
        padded[..bytes.len()].copy_from_slice(bytes);
        Ok(Asset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12(padded),
            issuer,
        }))
    } else {
        Err(())
    }
}

/// Convert an XDR `Asset` into the corresponding [`SacAssetIdentity`].
fn asset_to_identity(asset: &Asset) -> SacAssetIdentity {
    match asset {
        Asset::Native => SacAssetIdentity::Native,
        // Shared normalizer (task 0359 concern 3) so a SAC-wrapped asset's
        // `asset_id` matches the op / balance pipelines for the same asset — no
        // more per-pipeline identity fracture on malformed codes.
        Asset::CreditAlphanum4(a) => SacAssetIdentity::Credit {
            code: crate::asset_code::asset_code_str(&a.asset_code.0),
            issuer: a.issuer.0.to_string(),
        },
        Asset::CreditAlphanum12(a) => SacAssetIdentity::Credit {
            code: crate::asset_code::asset_code_str(&a.asset_code.0),
            issuer: a.issuer.0.to_string(),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests;
