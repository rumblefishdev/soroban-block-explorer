use super::*;
use serde_json::json;

const ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";
const G_FROM: &str = "GBLVLKGRDU66WLWY4XRORJXCC4LDZ347AQTUYBEPBABIZTVITW2OAGIP";
const G_TO: &str = "GADKLS7RS3OC2MXGEZXQA46JNF3FBVSTHTWLDPRF7TWI6GXVP4OUE3ZR";

fn addr(v: &str) -> Value {
    json!({ "type": "address", "value": v })
}
fn sym(v: &str) -> Value {
    json!({ "type": "sym", "value": v })
}
fn strv(v: &str) -> Value {
    json!({ "type": "string", "value": v })
}

#[test]
fn sac_transfer_yields_participants_and_classic_asset() {
    let d = derive_token_event(
        &json!([
            sym("transfer"),
            addr(G_FROM),
            addr(G_TO),
            strv(&format!("USDC:{ISSUER}"))
        ]),
        None,
    )
    .unwrap();
    assert_eq!(
        d.participant_strkeys,
        vec![G_FROM.to_string(), G_TO.to_string()]
    );
    assert_eq!(
        d.asset_id,
        Some(ids::asset_id(1, "USDC", ids::account_id(ISSUER), 0))
    );
}

#[test]
fn native_transfer_yields_native_asset() {
    let d = derive_token_event(
        &json!([sym("transfer"), addr(G_FROM), addr(G_TO), strv("native")]),
        None,
    )
    .unwrap();
    assert_eq!(d.asset_id, Some(ids::NATIVE_ASSET_ID));
}

#[test]
fn mint_yields_only_to_participant() {
    let d = derive_token_event(
        &json!([sym("mint"), addr(G_TO), strv(&format!("USDC:{ISSUER}"))]),
        None,
    )
    .unwrap();
    assert_eq!(d.participant_strkeys, vec![G_TO.to_string()]);
    assert!(d.asset_id.is_some());
}

#[test]
fn bespoke_contract_event_resolves_asset_to_emitting_contract() {
    // Bespoke transfer (no SEP-11 asset string): asset_id = the EMITTING
    // contract's surrogate (task 0393). Accounts still tx participants.
    const CTOKEN: &str = "CBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N";
    let d = derive_token_event(
        &json!([sym("transfer"), addr(G_FROM), addr(G_TO)]),
        Some(ids::contract_id(CTOKEN)),
    )
    .unwrap();
    assert_eq!(
        d.participant_strkeys,
        vec![G_FROM.to_string(), G_TO.to_string()]
    );
    assert_eq!(d.asset_id, Some(ids::contract_id(CTOKEN)));
}

#[test]
fn bespoke_event_without_emitting_contract_has_no_asset() {
    let d = derive_token_event(&json!([sym("transfer"), addr(G_FROM), addr(G_TO)]), None).unwrap();
    assert_eq!(d.asset_id, None);
}

#[test]
fn contract_address_participant_is_dropped() {
    // from = a C-contract address → not a G-account, dropped from participants.
    let d = derive_token_event(
        &json!([
            sym("transfer"),
            addr("CCONTRACTADDR"),
            addr(G_TO),
            strv("native")
        ]),
        None,
    )
    .unwrap();
    assert_eq!(d.participant_strkeys, vec![G_TO.to_string()]);
}

#[test]
fn non_token_event_is_none() {
    assert!(derive_token_event(&json!([sym("swap"), addr(G_FROM)]), None).is_none());
}
