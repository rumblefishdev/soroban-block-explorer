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
    ContractIdPreimage, CreateContractArgs, CreateContractArgsV2, Hash, HashIdPreimage,
    HashIdPreimageContractId, HostFunction, Limits, OperationBody, ScAddress,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, WriteXdr,
};

use crate::envelope::InnerTxRef;
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

    // -- Factory SAC: CreateContractHostFn carried inside auth entries --

    use stellar_xdr::{
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

    // ----------------------------------------------------------------------
    // Task 0294 — `sac_override_from_event_topics` (the shared SAC gate)
    // ----------------------------------------------------------------------

    const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    const NATIVE_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

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
}
