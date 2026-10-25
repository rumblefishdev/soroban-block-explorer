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
        AccountId::from_str("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN").unwrap();
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

// -- SAC identity from the SAC's own instance entry (task 0571) --

/// Mainnet SAC `CAIYL7WX…` (asset `BCB:GD7P2H4M…`), deployed at ledger
/// 64,951,791 by another contract's `create_stellar_asset_contract` call, so
/// its envelope carries no asset. Its instance entry as stored in
/// `contract_instances`, base64 `LedgerEntryData`.
pub(crate) const BCB_SAC_INSTANCE: &str = "AAAABgAAAAAAAAABEYX+10pEDGKE47WTck84RYp4oqlcwC2SCKi+E8xd0PcAAAAUAAAAAQAA\
    ABMAAAABAAAAAQAAAAMAAAAPAAAACE1FVEFEQVRBAAAAEQAAAAEAAAADAAAADwAAAAdkZWNp\
    bWFsAAAAAAMAAAAHAAAADwAAAARuYW1lAAAADgAAADxCQ0I6R0Q3UDJINE1TVlczVjZMMzRZ\
    Rlg3WVpCSU1XQU9TU1pWWEVMSkFOQlBLTURSTzM0RFlWMzdCQ0IAAAAPAAAABnN5bWJvbAAA\
    AAAADgAAAANCQ0IAAAAAEAAAAAEAAAABAAAADwAAAAVBZG1pbgAAAAAAABIAAAAAAAAAAP79\
    H4yVbbr5e+YLf+MhQywHSlmtyLSBoXqYOLt8Hiu/AAAAEAAAAAEAAAABAAAADwAAAAlBc3Nl\
    dEluZm8AAAAAAAAQAAAAAQAAAAIAAAAPAAAACUFscGhhTnVtNAAAAAAAABEAAAABAAAAAgAA\
    AA8AAAAKYXNzZXRfY29kZQAAAAAADgAAAARCQ0IAAAAADwAAAAZpc3N1ZXIAAAAAAA0AAAAg\
    /v0fjJVtuvl75gt/4yFDLAdKWa3ItIGhepg4u3weK78=";
pub(crate) const BCB_SAC: &str = "CAIYL7WXJJCAYYUE4O2ZG4SPHBCYU6FCVFOMALMSBCUL4E6MLXIPPQA7";
const BCB_ISSUER: &str = "GD7P2H4MSVW3V6L34YFX7YZBIMWAOSSZVXELJANBPKMDRO34DYV37BCB";

/// Mainnet XLM SAC instance, base64 `LedgerEntryData`.
const XLM_SAC_INSTANCE: &str = "AAAABgAAAAAAAAABJbT82FmuwvpjSEOMSJs8PBDJi20hvk/TyzDLaJU++XcAAAAUAAAAAQAA\
    ABMAAAABAAAAAQAAAAIAAAAPAAAACE1FVEFEQVRBAAAAEQAAAAEAAAADAAAADwAAAAdkZWNp\
    bWFsAAAAAAMAAAAHAAAADwAAAARuYW1lAAAADgAAAAZuYXRpdmUAAAAAAA8AAAAGc3ltYm9s\
    AAAAAAAOAAAABm5hdGl2ZQAAAAAAEAAAAAEAAAABAAAADwAAAAlBc3NldEluZm8AAAAAAAAQ\
    AAAAAQAAAAEAAAAPAAAABk5hdGl2ZQAA";

/// A stored `contract_instances.data_xdr` entry, decoded.
pub(crate) fn entry_data(b64: &str) -> stellar_xdr::LedgerEntryData {
    use base64::Engine;
    use stellar_xdr::{LedgerEntryData, Limits, ReadXdr};
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    LedgerEntryData::from_xdr(bytes, Limits::none()).unwrap()
}

/// The typed-JSON `val` of a stored instance entry, through the same encoder
/// the ingest path uses.
fn instance_val(b64: &str) -> serde_json::Value {
    let stellar_xdr::LedgerEntryData::ContractData(cd) = entry_data(b64) else {
        panic!("not a ContractData entry");
    };
    crate::scval::scval_to_typed_json(&cd.val)
}

#[test]
fn sac_identity_from_instance_reads_a_contract_deployed_credit_sac() {
    let net = network_id(MAINNET_PASSPHRASE);
    assert_eq!(
        sac_identity_from_instance(BCB_SAC, &instance_val(BCB_SAC_INSTANCE), &net),
        Some(SacAssetIdentity::Credit {
            code: "BCB".into(),
            issuer: BCB_ISSUER.into()
        })
    );
}

#[test]
fn sac_identity_from_instance_reads_the_native_sac() {
    let net = network_id(MAINNET_PASSPHRASE);
    assert_eq!(
        sac_identity_from_instance(NATIVE_SAC, &instance_val(XLM_SAC_INSTANCE), &net),
        Some(SacAssetIdentity::Native)
    );
}

/// The name in storage must derive to the contract holding it — the same
/// proof the event path demands. BCB's storage under the XLM SAC's address
/// is not BCB's SAC.
#[test]
fn sac_identity_from_instance_refuses_a_name_that_does_not_derive_to_the_contract() {
    let net = network_id(MAINNET_PASSPHRASE);
    assert_eq!(
        sac_identity_from_instance(NATIVE_SAC, &instance_val(BCB_SAC_INSTANCE), &net),
        None
    );
    assert_eq!(
        sac_identity_from_instance(
            BCB_SAC,
            &instance_val(BCB_SAC_INSTANCE),
            &network_id(TESTNET_PASSPHRASE)
        ),
        None
    );
}

/// 86 early testnet SAC instances carry no storage (measured 2026-10-16).
#[test]
fn sac_identity_from_instance_is_none_without_storage() {
    let net = network_id(MAINNET_PASSPHRASE);
    let empty = serde_json::json!({
        "type": "contract_instance",
        "value": { "executable": { "type": "stellar_asset" }, "storage": null }
    });
    assert_eq!(sac_identity_from_instance(BCB_SAC, &empty, &net), None);
}
