use serde_json::json;
use stellar_xdr::{
    ContractId, Hash, ScAddress, ScBytes, ScMapEntry, ScString, ScSymbol, ScVal, ScVec,
};

use super::*;

/// Real mainnet `executable_update` topics from contract CCABO2IQ… (first
/// upgrade, ledger 55363489): old `8b89f74f…`, new `55827b34…`. The fn must
/// return the NEW hash.
#[test]
fn extracts_new_wasm_hash_from_executable_update_topics() {
    let topics: Value = serde_json::from_str(
        r#"[{"type":"sym","value":"executable_update"},{"type":"vec","value":[{"type":"sym","value":"Wasm"},{"type":"bytes","value":"i4n3TxyvZkB6DzwJPtmvBXoW5c9VvuVtd56kVKbDCxw="}]},{"type":"vec","value":[{"type":"sym","value":"Wasm"},{"type":"bytes","value":"VYJ7NLoW/zBUeWZF4L0xpQ6HGUzBI9zdjg8i56LMnZg="}]}]"#,
    )
    .unwrap();
    let Some(ExecutableUpdate::Wasm(got)) = extract_executable_update(&topics) else {
        panic!("a Wasm upgrade must decode as one");
    };
    let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        got_hex,
        "55827b34ba16ff3054796645e0bd31a50e87194cc123dcdd8e0f22e7a2cc9d98"
    );
}

#[test]
fn ignores_non_executable_update_event() {
    // A `transfer` event must never yield a hash — guards the backfill from
    // mis-firing on the ~9.25B non-upgrade events.
    let topics: Value = serde_json::from_str(
        r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GAHWFCCYCDSSEYZRVV4446KBDP6X2JR2T56TG4OGANS3AS3NCILOCGU5"}]"#,
    )
    .unwrap();
    assert_eq!(extract_executable_update(&topics), None);
}

/// Protocol 28 / CAP-85: an upgrade can now point the contract at code owned
/// by ANOTHER contract, and it emits the very same `executable_update` event.
///
/// The topic shape is given by the CAP:
/// `SCVec([SCSymbol("ExternalRef"), SCMap([(owner, SCAddress), (tag, SCString)])])`.
///
/// The reference has to come back INTACT, because the writer clears
/// `wasm_hash` on it. Reading this as "nothing happened" is what would leave
/// the contract serving the hash it ran BEFORE the upgrade — the stale-hash
/// defect of tasks 0320/0326, reached through a door the compiler cannot
/// watch, since the topics are JSON by this point rather than a Rust enum.
#[test]
fn an_external_ref_upgrade_decodes_to_the_reference_it_sets() {
    let external_ref = ScVal::Vec(Some(
        vec![
            ScVal::Symbol(ScSymbol::try_from(b"ExternalRef".to_vec()).unwrap()),
            ScVal::Map(Some(
                vec![
                    ScMapEntry {
                        key: ScVal::Symbol(ScSymbol::try_from(b"owner".to_vec()).unwrap()),
                        val: ScVal::Address(ScAddress::Contract(ContractId(Hash([0x11; 32])))),
                    },
                    ScMapEntry {
                        key: ScVal::Symbol(ScSymbol::try_from(b"tag".to_vec()).unwrap()),
                        val: ScVal::String(ScString::try_from(b"fleet-v2".to_vec()).unwrap()),
                    },
                ]
                .try_into()
                .unwrap(),
            )),
        ]
        .try_into()
        .unwrap(),
    ));

    let topics = json!([
        crate::scval::scval_to_typed_json(&ScVal::Symbol(
            ScSymbol::try_from(b"executable_update".to_vec()).unwrap()
        )),
        crate::scval::scval_to_typed_json(&ScVal::Vec(Some(
            vec![
                ScVal::Symbol(ScSymbol::try_from(b"Wasm".to_vec()).unwrap()),
                ScVal::Bytes(ScBytes::try_from(vec![0xAA; 32]).unwrap()),
            ]
            .try_into()
            .unwrap(),
        ))),
        crate::scval::scval_to_typed_json(&external_ref),
    ]);

    let Some(ExecutableUpdate::ExternalRef { owner, tag }) = extract_executable_update(&topics)
    else {
        panic!("an ExternalRef upgrade must decode as one, not as `None`");
    };
    assert!(
        owner.starts_with('C'),
        "owner is a contract StrKey: {owner}"
    );
    assert_eq!(tag, "fleet-v2");
}

#[test]
fn ignores_non_wasm_executable() {
    // Defensive: a StellarAsset (SAC) executable carries no wasm hash.
    let topics: Value = serde_json::from_str(
        r#"[{"type":"sym","value":"executable_update"},{"type":"vec","value":[{"type":"sym","value":"StellarAsset"}]},{"type":"vec","value":[{"type":"sym","value":"StellarAsset"}]}]"#,
    )
    .unwrap();
    assert_eq!(extract_executable_update(&topics), None);
}

/// Round-trip through the REAL `scval_to_typed_json` encoder (not a
/// hand-written base64 literal) so the parser stays pinned to how the
/// indexer actually serializes `Bytes`/`Vec`/`Symbol` topics — a future
/// change to that encoding breaks this test in CI instead of silently
/// regressing upgrade detection to "unparseable".
#[test]
fn extracts_new_hash_via_real_scval_encoding() {
    use crate::scval::scval_to_typed_json;
    let sym = |s: &str| ScVal::Symbol(ScSymbol::try_from(s.as_bytes().to_vec()).unwrap());
    let exec = |b: u8| {
        ScVal::Vec(Some(
            ScVec::try_from(vec![
                sym("Wasm"),
                ScVal::Bytes(ScBytes::try_from(vec![b; 32]).unwrap()),
            ])
            .unwrap(),
        ))
    };
    let topics = serde_json::json!([
        scval_to_typed_json(&sym("executable_update")),
        scval_to_typed_json(&exec(0x11)),
        scval_to_typed_json(&exec(0x99)),
    ]);
    assert_eq!(
        extract_executable_update(&topics),
        Some(ExecutableUpdate::Wasm([0x99u8; 32]))
    );
}
