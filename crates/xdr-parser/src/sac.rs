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
use std::collections::HashSet;

use domain::TokenAssetType;
use sha2::{Digest, Sha256};
use stellar_xdr::curr::{
    AccountId, AlphaNum4, AlphaNum12, Asset, AssetCode4, AssetCode12, ContractId,
    ContractIdPreimage, CreateContractArgs, CreateContractArgsV2, Hash, HashIdPreimage,
    HashIdPreimageContractId, HostFunction, Limits, OperationBody, ScAddress,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, WriteXdr,
};
use tracing::{instrument, warn};

use crate::envelope::InnerTxRef;
use crate::error::{ParseError, ParseErrorKind};
use crate::types::{EventSource, ExtractedAsset, ExtractedEvent, SacAssetIdentity};

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

/// Forward-derived SAC `(contract_id, identity)` pair (task 0218).
///
/// Produced by [`derive_sac_overrides_from_assets`] for every observed
/// classic-credit / native asset; consumed by the indexer persist path
/// to flip `is_sac=true` + `sac_asset` on pre-existing SAC contracts
/// whose `create_contract` op happened before the indexed window and
/// therefore left the row as a skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SacOverride {
    /// Derived SAC `C…` StrKey for the asset.
    pub contract_id: String,
    /// Asset identity carried into `assets.asset_code` / `.issuer_id`
    /// for the corresponding `assets` row.
    pub identity: SacAssetIdentity,
}

/// Derive the SAC `contract_id` for every observed classic / native asset.
///
/// Native and classic-credit assets each have a single deterministic SAC
/// contract_id (per network). The derivation is the same as
/// [`derive_sac_contract_id`] applied to `ContractIdPreimage::Asset(...)`;
/// this helper just walks the asset list and emits one [`SacOverride`]
/// per derivable entry.
///
/// `TokenAssetType::Sac` and `TokenAssetType::Soroban` assets are skipped:
///
/// - `Sac` already carries the observed `contract_id` from the
///   in-window `create_contract` op (and its `soroban_contracts` row is
///   already `is_sac=true`).
/// - `Soroban` is a bespoke contract token with no SAC mapping.
///
/// Derivation errors (malformed issuer StrKey, XDR serialize failure)
/// are `warn!`-logged and skipped — they must not fail the persist tx.
#[instrument(skip(assets), fields(asset_count = assets.len()))]
pub fn derive_sac_overrides_from_assets(
    assets: &[ExtractedAsset],
    network_passphrase: &str,
) -> Vec<SacOverride> {
    let net_id = network_id(network_passphrase);
    let mut out: Vec<SacOverride> = Vec::new();

    for asset in assets {
        match asset.asset_type {
            TokenAssetType::Native => {
                let preimage = ContractIdPreimage::Asset(Asset::Native);
                match derive_sac_contract_id(&preimage, &net_id) {
                    Ok(contract_id) => out.push(SacOverride {
                        contract_id,
                        identity: SacAssetIdentity::Native,
                    }),
                    Err(e) => warn!(
                        target: "xdr_parser::sac",
                        error = %e.message,
                        "SAC derive failed for native asset",
                    ),
                }
            }
            TokenAssetType::ClassicCredit => {
                let Some(code) = asset.asset_code.as_ref() else {
                    warn!(
                        target: "xdr_parser::sac",
                        "classic_credit asset missing asset_code; skipping SAC derive",
                    );
                    continue;
                };
                let Some(issuer) = asset.issuer_address.as_ref() else {
                    warn!(
                        target: "xdr_parser::sac",
                        code = %code,
                        "classic_credit asset missing issuer; skipping SAC derive",
                    );
                    continue;
                };
                let issuer_acct = match AccountId::from_str(issuer) {
                    Ok(acct) => acct,
                    Err(_) => {
                        warn!(
                            target: "xdr_parser::sac",
                            code = %code,
                            issuer = %issuer,
                            "invalid issuer StrKey; skipping SAC derive",
                        );
                        continue;
                    }
                };
                let xdr_asset = match build_credit_asset(code, issuer_acct) {
                    Ok(a) => a,
                    Err(()) => {
                        warn!(
                            target: "xdr_parser::sac",
                            code = %code,
                            "asset_code too long for AlphaNum12; skipping SAC derive",
                        );
                        continue;
                    }
                };
                let preimage = ContractIdPreimage::Asset(xdr_asset);
                match derive_sac_contract_id(&preimage, &net_id) {
                    Ok(contract_id) => out.push(SacOverride {
                        contract_id,
                        identity: SacAssetIdentity::Credit {
                            code: code.clone(),
                            issuer: issuer.clone(),
                        },
                    }),
                    Err(e) => warn!(
                        target: "xdr_parser::sac",
                        code = %code,
                        issuer = %issuer,
                        error = %e.message,
                        "SAC derive failed for classic_credit asset",
                    ),
                }
            }
            // SAC contracts already carry their observed contract_id from the
            // in-window create_contract op; no forward derivation needed.
            TokenAssetType::Sac => {}
            // Soroban-native tokens are bespoke contracts with no SAC mapping.
            TokenAssetType::Soroban => {}
        }
    }

    out
}

/// Forward-derive SAC overrides from classic-asset SAC *events* (task 0294).
///
/// Un-deployed SACs surface `transfer`/`mint`/`burn`/`clawback`/`set_authorized`
/// events under their deterministic SAC `contract_id` — via direct SAC
/// host-function invocation (Protocol 20+) pre-P23, and additionally via CAP-67
/// unified asset events post-P23 — carrying the asset `CODE:ISSUER` (or
/// `native`) as an `ScVal::String` in the LAST topic.
/// [`detect_classic_credit_assets`](crate::detect_classic_credit_assets) reads
/// only trustline changes, so payment/transfer-only SACs are never
/// forward-derived and persist as `is_sac=false` orphans.
///
/// **Crypto-match gate (task 0294 C1):** an override is emitted ONLY when the
/// emitter `contract_id` equals the SAC `contract_id` derived from the topic
/// asset (`emitter == derive_sac(asset)`). This rejects bespoke WASM contracts
/// that merely emit a SAC-shaped event with an asset-string topic — their id
/// never equals the derived SAC id — so they are never mislabeled `is_sac`.
pub fn derive_sac_overrides_from_events(
    events: &[ExtractedEvent],
    network_passphrase: &str,
) -> Vec<SacOverride> {
    let net_id = network_id(network_passphrase);
    let mut out: Vec<SacOverride> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for event in events {
        // Diagnostic events are byte-identical mirrors of the consensus per-op
        // events (task 0182) — skip to avoid redundant work.
        if event.source == EventSource::Diagnostic {
            continue;
        }
        let Some(emitter) = event.contract_id.as_deref() else {
            continue;
        };
        let Some(ov) = sac_override_from_event_topics(emitter, &event.topics, &net_id) else {
            continue;
        };
        if seen.insert(ov.contract_id.clone()) {
            out.push(ov);
        }
    }

    out
}

/// Crypto-match-gated SAC override for a single event's `(emitter, topics)`.
///
/// Returns `Some` only when `emitter` IS the SAC contract for the asset carried
/// in the event's LAST topic (`emitter == derive_sac(asset)`) and the event's
/// first topic is a SAC-control signature. This is the shared core of the live
/// path ([`derive_sac_overrides_from_events`]) and the batch orphan-relabel
/// pass (task 0294) — both must apply the identical gate so a bespoke WASM
/// emitter of a SAC-shaped event is never mislabeled `is_sac`.
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
    let signature = topic_symbol_value(&topics[0]).to_ascii_lowercase();
    if !SAC_CONTROL_EVENT_SIGNATURES.contains(&signature.as_str()) {
        return None;
    }
    // The SEP-11 asset string rides in the LAST topic across every SAC event
    // shape (transfer/mint/burn/clawback/set_authorized) — both the pre-P23
    // direct-invocation form and the post-P23 CAP-67 form.
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

/// Classic-asset SAC control-event signatures. `transfer`/`mint`/`burn` are
/// shared with bespoke tokens, but `clawback`/`set_authorized` are SAC-only
/// (a custom Soroban NFT never emits them). The crypto-match gate in
/// [`derive_sac_overrides_from_events`] is what makes the shared signatures
/// safe to act on.
const SAC_CONTROL_EVENT_SIGNATURES: &[&str] =
    &["transfer", "mint", "burn", "clawback", "set_authorized"];

/// Extract an `ScVal::Symbol` string from a tagged JSON topic (`type: "sym"`).
fn topic_symbol_value(topic: &serde_json::Value) -> String {
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
        Asset::CreditAlphanum4(a) => SacAssetIdentity::Credit {
            code: asset_code_to_string(&a.asset_code.0),
            issuer: a.issuer.0.to_string(),
        },
        Asset::CreditAlphanum12(a) => SacAssetIdentity::Credit {
            code: asset_code_to_string(&a.asset_code.0),
            issuer: a.issuer.0.to_string(),
        },
    }
}

fn asset_code_to_string(bytes: &[u8]) -> String {
    // Asset codes are zero-padded to 4 or 12 bytes; strip trailing NULs so
    // "USDC\0\0\0\0" round-trips to "USDC".
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn push_preimage_identity(
    preimage: &ContractIdPreimage,
    network_id: &[u8; 32],
    out: &mut Vec<(String, SacAssetIdentity)>,
) {
    let ContractIdPreimage::Asset(asset) = preimage else {
        return;
    };
    let identity = asset_to_identity(asset);
    match derive_sac_contract_id(preimage, network_id) {
        Ok(contract_id) => out.push((contract_id, identity)),
        Err(e) => tracing::warn!(error = %e.message, "derive_sac_contract_id failed"),
    }
}

fn walk_auth_node(
    node: &SorobanAuthorizedInvocation,
    network_id: &[u8; 32],
    out: &mut Vec<(String, SacAssetIdentity)>,
) {
    match &node.function {
        SorobanAuthorizedFunction::CreateContractHostFn(CreateContractArgs {
            contract_id_preimage,
            ..
        }) => push_preimage_identity(contract_id_preimage, network_id, out),
        SorobanAuthorizedFunction::CreateContractV2HostFn(CreateContractArgsV2 {
            contract_id_preimage,
            ..
        }) => push_preimage_identity(contract_id_preimage, network_id, out),
        SorobanAuthorizedFunction::ContractFn(_) => {}
    }
    for child in node.sub_invocations.iter() {
        walk_auth_node(child, network_id, out);
    }
}

/// Collect all SAC `(contract_id, identity)` pairs reachable from a single
/// transaction envelope — both top-level `CreateContract` host-function
/// operations AND `CreateContractHostFn` auth entries (factory pattern).
///
/// Each `contract_id` is derived from the preimage via
/// [`derive_sac_contract_id`] (stellar-core convention), so downstream
/// persistence can key off a deterministic, batch-independent identifier
/// rather than `tx_hash` correlation.
pub fn extract_sac_identities(
    envelope: &InnerTxRef<'_>,
    network_id: &[u8; 32],
) -> Vec<(String, SacAssetIdentity)> {
    let ops = match envelope {
        InnerTxRef::V0(tx) => tx.operations.as_slice(),
        InnerTxRef::V1(tx) => tx.operations.as_slice(),
    };
    let mut out = Vec::new();
    for op in ops {
        let OperationBody::InvokeHostFunction(ref invoke) = op.body else {
            continue;
        };
        match &invoke.host_function {
            HostFunction::CreateContract(args) => {
                push_preimage_identity(&args.contract_id_preimage, network_id, &mut out);
            }
            HostFunction::CreateContractV2(args) => {
                push_preimage_identity(&args.contract_id_preimage, network_id, &mut out);
            }
            _ => {}
        }
        for auth_entry in invoke.auth.iter() {
            walk_auth_node(&auth_entry.root_invocation, network_id, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mainnet network_id is a well-known hex string; drift in SHA2 or
    /// passphrase definition would be caught here before anything else
    /// misbehaves.
    #[test]
    fn mainnet_network_id_matches_known_hex() {
        assert_eq!(
            hex::encode(network_id(MAINNET_PASSPHRASE)),
            "7ac33997544e3175d266bd022439b22cdb16508c01163f26e5cb2a3e1045a979"
        );
    }

    #[test]
    fn testnet_network_id_matches_known_hex() {
        assert_eq!(
            hex::encode(network_id(TESTNET_PASSPHRASE)),
            "cee0302d59844d32bdca915c8203dd44b33fbb7edc19051ea37abedf28ecd472"
        );
    }

    #[test]
    fn passphrase_lookup_accepts_common_aliases() {
        assert_eq!(passphrase_for("mainnet"), Some(MAINNET_PASSPHRASE));
        assert_eq!(passphrase_for("MAINNET"), Some(MAINNET_PASSPHRASE));
        assert_eq!(passphrase_for("public"), Some(MAINNET_PASSPHRASE));
        assert_eq!(passphrase_for("testnet"), Some(TESTNET_PASSPHRASE));
        assert_eq!(passphrase_for("bogus"), None);
    }

    /// XLM-SAC on mainnet is a published constant across the Stellar
    /// ecosystem (Horizon, SDK, Stellar Expert). Regression-guards the
    /// derivation against any change in XDR layout or hashing inputs.
    #[test]
    fn xlm_sac_mainnet_contract_id() {
        let net = network_id(MAINNET_PASSPHRASE);
        let preimage = ContractIdPreimage::Asset(Asset::Native);
        let cid = derive_sac_contract_id(&preimage, &net).unwrap();
        assert_eq!(
            cid,
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        );
    }

    /// Circle USDC mainnet SAC: issuer `GA5ZSEJY...KZVN`, code `USDC`.
    #[test]
    fn usdc_sac_mainnet_contract_id() {
        use core::str::FromStr;
        let issuer =
            AccountId::from_str("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
                .unwrap();
        let asset = Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USDC"),
            issuer,
        });

        let net = network_id(MAINNET_PASSPHRASE);
        let preimage = ContractIdPreimage::Asset(asset);
        let cid = derive_sac_contract_id(&preimage, &net).unwrap();
        assert_eq!(
            cid,
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
        );
    }

    // ----------------------------------------------------------------------
    // Task 0218 — `derive_sac_overrides_from_assets`
    // ----------------------------------------------------------------------

    fn native_asset_input() -> ExtractedAsset {
        ExtractedAsset {
            asset_type: TokenAssetType::Native,
            asset_code: None,
            issuer_address: None,
            contract_id: None,
            name: None,
            total_supply: None,
            holder_count: None,
        }
    }

    fn classic_credit_asset_input(code: &str, issuer: &str) -> ExtractedAsset {
        ExtractedAsset {
            asset_type: TokenAssetType::ClassicCredit,
            asset_code: Some(code.to_string()),
            issuer_address: Some(issuer.to_string()),
            contract_id: None,
            name: None,
            total_supply: None,
            holder_count: None,
        }
    }

    #[test]
    fn overrides_native_xlm_maps_to_mainnet_sac() {
        let overrides =
            derive_sac_overrides_from_assets(&[native_asset_input()], MAINNET_PASSPHRASE);
        assert_eq!(overrides.len(), 1);
        assert_eq!(
            overrides[0].contract_id,
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        );
        assert_eq!(overrides[0].identity, SacAssetIdentity::Native);
    }

    #[test]
    fn overrides_usdc_classic_credit_maps_to_mainnet_sac() {
        let usdc = classic_credit_asset_input(
            "USDC",
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        );
        let overrides = derive_sac_overrides_from_assets(&[usdc], MAINNET_PASSPHRASE);
        assert_eq!(overrides.len(), 1);
        assert_eq!(
            overrides[0].contract_id,
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
        );
        assert_eq!(
            overrides[0].identity,
            SacAssetIdentity::Credit {
                code: "USDC".into(),
                issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
            }
        );
    }

    // ----------------------------------------------------------------------
    // Task 0294 — `derive_sac_overrides_from_events` (un-deployed SAC labeling)
    // ----------------------------------------------------------------------

    const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    const NATIVE_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

    /// Build a SAC-shaped event: `[sym(sig), addr, addr, <asset_topic>]`.
    /// The asset rides in the LAST topic across every SAC event shape.
    fn make_sac_event(emitter: &str, sig: &str, asset_topic: serde_json::Value) -> ExtractedEvent {
        ExtractedEvent {
            transaction_hash: "deadbeef".into(),
            event_type: domain::ContractEventType::Contract,
            source: EventSource::PerOp,
            contract_id: Some(emitter.into()),
            topics: serde_json::json!([
                {"type": "sym", "value": sig},
                {"type": "address", "value": "GFROM"},
                {"type": "address", "value": "GTO"},
                asset_topic,
            ]),
            data: serde_json::json!({"type": "i128", "value": "8441727124"}),
            event_index: 0,
            ledger_sequence: 60_000_000,
            created_at: 1_700_000_000,
        }
    }

    fn usdc_asset_topic() -> serde_json::Value {
        serde_json::json!({"type": "string", "value": "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"})
    }

    #[test]
    fn events_emitter_is_real_sac_yields_token_override() {
        let ev = make_sac_event(USDC_SAC, "transfer", usdc_asset_topic());
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].contract_id, USDC_SAC);
        assert_eq!(
            out[0].identity,
            SacAssetIdentity::Credit {
                code: "USDC".into(),
                issuer: USDC_ISSUER.into(),
            }
        );
    }

    /// C1 guardrail: a bespoke WASM contract emits the SAC transfer shape with
    /// a real asset string, but its own id != derive_sac(asset). It MUST NOT be
    /// flipped to `is_sac=true` (verified: 3 such mainnet contracts exist).
    #[test]
    fn events_crypto_match_gate_rejects_non_sac_emitter() {
        // emitter is the *native* SAC id, asset string is USDC → mismatch.
        let ev = make_sac_event(NATIVE_SAC, "transfer", usdc_asset_topic());
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert!(
            out.is_empty(),
            "non-SAC emitter must be rejected, got {out:?}"
        );
    }

    #[test]
    fn events_native_asset_yields_native_override() {
        let ev = make_sac_event(
            NATIVE_SAC,
            "mint",
            serde_json::json!({"type": "string", "value": "native"}),
        );
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].contract_id, NATIVE_SAC);
        assert_eq!(out[0].identity, SacAssetIdentity::Native);
    }

    #[test]
    fn events_clawback_signature_is_sac_control() {
        // clawback is SAC/asset-only — a bespoke NFT never emits it.
        let ev = make_sac_event(USDC_SAC, "clawback", usdc_asset_topic());
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].contract_id, USDC_SAC);
    }

    #[test]
    fn events_non_sac_signature_ignored() {
        let ev = make_sac_event(USDC_SAC, "approve", usdc_asset_topic());
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert!(out.is_empty());
    }

    #[test]
    fn events_same_sac_deduped_to_one_override() {
        let evs = [
            make_sac_event(USDC_SAC, "transfer", usdc_asset_topic()),
            make_sac_event(USDC_SAC, "mint", usdc_asset_topic()),
        ];
        let out = derive_sac_overrides_from_events(&evs, MAINNET_PASSPHRASE);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn events_diagnostic_source_skipped() {
        let mut ev = make_sac_event(USDC_SAC, "transfer", usdc_asset_topic());
        ev.source = EventSource::Diagnostic;
        let out = derive_sac_overrides_from_events(&[ev], MAINNET_PASSPHRASE);
        assert!(out.is_empty());
    }

    #[test]
    fn helper_override_from_topics_positive_and_gate() {
        let net = network_id(MAINNET_PASSPHRASE);
        let topics = serde_json::json!([
            {"type": "sym", "value": "transfer"},
            {"type": "address", "value": "GFROM"},
            {"type": "address", "value": "GTO"},
            {"type": "string", "value": "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"},
        ]);
        // positive: emitter IS the USDC SAC
        let ov = sac_override_from_event_topics(USDC_SAC, &topics, &net);
        assert_eq!(ov.expect("override").contract_id, USDC_SAC);
        // gate: wrong emitter (native SAC id) → rejected
        assert!(sac_override_from_event_topics(NATIVE_SAC, &topics, &net).is_none());
    }

    #[test]
    fn overrides_alphanum12_code_uses_credit_alphanum12_path() {
        // Codes 5–12 bytes select `CreditAlphanum12`. Compare against a
        // direct `derive_sac_contract_id` call with the same XDR shape to
        // pin down the wrapper's behaviour.
        let issuer_strkey = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        let asset = classic_credit_asset_input("LONGCODE", issuer_strkey);
        let overrides = derive_sac_overrides_from_assets(&[asset], MAINNET_PASSPHRASE);
        assert_eq!(overrides.len(), 1);

        let issuer = AccountId::from_str(issuer_strkey).unwrap();
        let mut padded = [0u8; 12];
        padded[..8].copy_from_slice(b"LONGCODE");
        let direct = derive_sac_contract_id(
            &ContractIdPreimage::Asset(Asset::CreditAlphanum12(AlphaNum12 {
                asset_code: AssetCode12(padded),
                issuer,
            })),
            &network_id(MAINNET_PASSPHRASE),
        )
        .unwrap();
        assert_eq!(overrides[0].contract_id, direct);
    }

    #[test]
    fn overrides_sac_and_soroban_assets_are_skipped() {
        // SAC already carries its observed contract_id; Soroban-native
        // has no SAC mapping. Both contribute zero overrides.
        let sac = ExtractedAsset {
            asset_type: TokenAssetType::Sac,
            asset_code: Some("USDC".into()),
            issuer_address: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
            contract_id: Some("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".into()),
            name: None,
            total_supply: None,
            holder_count: None,
        };
        let soroban = ExtractedAsset {
            asset_type: TokenAssetType::Soroban,
            asset_code: None,
            issuer_address: None,
            contract_id: Some("CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY".into()),
            name: None,
            total_supply: None,
            holder_count: None,
        };
        let overrides = derive_sac_overrides_from_assets(&[sac, soroban], MAINNET_PASSPHRASE);
        assert!(overrides.is_empty());
    }

    #[test]
    fn overrides_classic_credit_with_invalid_issuer_strkey_is_skipped() {
        // Malformed issuer must not panic the derive; warn-log + skip.
        let asset = classic_credit_asset_input("USDC", "not-a-valid-strkey");
        let overrides = derive_sac_overrides_from_assets(&[asset], MAINNET_PASSPHRASE);
        assert!(overrides.is_empty());
    }

    #[test]
    fn overrides_classic_credit_missing_code_or_issuer_is_skipped() {
        let no_code = ExtractedAsset {
            asset_type: TokenAssetType::ClassicCredit,
            asset_code: None,
            issuer_address: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
            contract_id: None,
            name: None,
            total_supply: None,
            holder_count: None,
        };
        let no_issuer = ExtractedAsset {
            asset_type: TokenAssetType::ClassicCredit,
            asset_code: Some("USDC".into()),
            issuer_address: None,
            contract_id: None,
            name: None,
            total_supply: None,
            holder_count: None,
        };
        let overrides = derive_sac_overrides_from_assets(&[no_code, no_issuer], MAINNET_PASSPHRASE);
        assert!(overrides.is_empty());
    }

    #[test]
    fn overrides_emits_one_per_observed_asset_in_input_order() {
        // Deterministic order — important for the persist UPDATE binding
        // (UNNEST aligns positionally).
        let assets = vec![
            native_asset_input(),
            classic_credit_asset_input(
                "USDC",
                "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
            ),
        ];
        let overrides = derive_sac_overrides_from_assets(&assets, MAINNET_PASSPHRASE);
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].identity, SacAssetIdentity::Native);
        assert!(matches!(
            overrides[1].identity,
            SacAssetIdentity::Credit { .. }
        ));
    }

    // -- Factory SAC: CreateContractHostFn carried inside auth entries --

    use stellar_xdr::curr::{
        ContractExecutable, InvokeContractArgs, InvokeHostFunctionOp, Memo, MuxedAccount,
        Operation, Preconditions, ScSymbol, SequenceNumber, SorobanAuthorizationEntry,
        SorobanCredentials, Transaction, TransactionExt, Uint256, VecM,
    };

    /// Build a single-operation V1 transaction whose only operation is an
    /// InvokeHostFunction call to a factory contract with the supplied
    /// auth-entry root invocation. Surface mirrors `invocation::tests::build_v1_tx`.
    fn build_factory_tx(root_invocation: SorobanAuthorizedInvocation) -> Transaction {
        let factory_call = HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: ScAddress::Contract(ContractId(Hash([0xFA; 32]))),
            function_name: ScSymbol::try_from(b"deploy_pair".to_vec()).unwrap(),
            args: VecM::default(),
        });
        let auth = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation,
        };
        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: factory_call,
                auth: vec![auth].try_into().unwrap(),
            }),
        };
        Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0xAA; 32])),
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![op].try_into().unwrap(),
            ext: TransactionExt::V0,
        }
    }

    fn create_contract_host_fn_node(asset: Asset) -> SorobanAuthorizedInvocation {
        SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::CreateContractHostFn(CreateContractArgs {
                contract_id_preimage: ContractIdPreimage::Asset(asset),
                executable: ContractExecutable::StellarAsset,
            }),
            sub_invocations: VecM::default(),
        }
    }

    /// Top-level factory pattern: auth entry's root invocation IS the
    /// CreateContractHostFn. Stellar SDK / soroban-cli emits this shape
    /// for direct sac-wrap invocations.
    #[test]
    fn extract_sac_identities_from_auth_entry_root_create_contract() {
        let tx = build_factory_tx(create_contract_host_fn_node(Asset::Native));
        let inner = InnerTxRef::V1(&tx);

        let net = network_id(MAINNET_PASSPHRASE);
        let pairs = extract_sac_identities(&inner, &net);

        assert_eq!(
            pairs.len(),
            1,
            "auth-entry root CreateContractHostFn picked up"
        );
        assert_eq!(pairs[0].1, SacAssetIdentity::Native);
        assert_eq!(
            pairs[0].0, "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
            "deterministic XLM-SAC contract_id derived even when the \
             CreateContractHostFn lives in auth, not in a top-level operation"
        );
    }

    /// Deep factory pattern: the auth entry's root is a regular ContractFn
    /// (the factory's `deploy_pair` entrypoint), with the actual
    /// CreateContractHostFn nested as a sub_invocation. Mirrors how LP /
    /// AMM factories surface their child SAC deploys.
    #[test]
    fn extract_sac_identities_from_nested_auth_sub_invocation() {
        use core::str::FromStr;
        let issuer =
            AccountId::from_str("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
                .unwrap();
        let usdc = Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USDC"),
            issuer,
        });
        let nested_create = create_contract_host_fn_node(usdc);

        let factory_root = SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: ScAddress::Contract(ContractId(Hash([0xFA; 32]))),
                function_name: ScSymbol::try_from(b"deploy_pair".to_vec()).unwrap(),
                args: VecM::default(),
            }),
            sub_invocations: vec![nested_create].try_into().unwrap(),
        };

        let tx = build_factory_tx(factory_root);
        let inner = InnerTxRef::V1(&tx);

        let net = network_id(MAINNET_PASSPHRASE);
        let pairs = extract_sac_identities(&inner, &net);

        assert_eq!(pairs.len(), 1, "nested CreateContractHostFn discovered");
        assert!(matches!(pairs[0].1, SacAssetIdentity::Credit { ref code, .. } if code == "USDC"));
        assert_eq!(
            pairs[0].0, "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            "USDC mainnet SAC contract_id derived from nested auth invocation"
        );
    }
}
