use super::*;
use crate::scval::scval_to_typed_json;
use serde_json::json;
use stellar_xdr::{
    ContractExecutable, ContractExecutableExternalRef, ContractId, Hash, ScAddress, ScBytes,
    ScContractInstance, ScString, ScVal,
};

const OWNER: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAM";

fn change(change_type: &str, key: Value, val: Value) -> ExtractedLedgerEntryChange {
    ExtractedLedgerEntryChange {
        transaction_hash: "ab".repeat(32),
        change_type: change_type.to_string(),
        entry_type: "contract_data".to_string(),
        key: json!({ "contract": OWNER, "key": key, "durability": "persistent" }),
        data: Some(json!({
            "contract": OWNER,
            "key": key,
            "durability": "persistent",
            "val": val,
        })),
        change_index: 0,
        operation_index: Some(0),
        ledger_sequence: 64_400_000,
        created_at: 1_789_000_000,
        token_metadata: None,
    }
}

/// The tag key and the hash value both go through the REAL encoder rather than
/// hand-written JSON, so a change to how `ScVal` is serialised breaks this test
/// instead of silently regressing the extractor to "matches nothing".
fn tag_key(tag: &str) -> Value {
    scval_to_typed_json(&ScVal::ExecutableTag(
        ScString::try_from(tag.as_bytes().to_vec()).unwrap(),
    ))
}

fn hash_value(byte: u8) -> Value {
    scval_to_typed_json(&ScVal::Bytes(ScBytes::try_from(vec![byte; 32]).unwrap()))
}

#[test]
fn extracts_the_mapping_an_owner_writes() {
    let changes = vec![change("created", tag_key("fleet-v2"), hash_value(0xAB))];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].owner, OWNER);
    assert_eq!(refs[0].tag, "fleet-v2");
    assert_eq!(refs[0].wasm_hash, "ab".repeat(32));
    assert_eq!(refs[0].ledger_sequence, 64_400_000);
}

#[test]
fn a_re_point_is_just_the_new_value() {
    // The owner swapping the fleet's code: one `updated` change, and the whole
    // fleet follows it. No member contract is touched, which is exactly why
    // this mapping cannot be denormalised into their rows.
    let changes = vec![change("updated", tag_key("fleet-v2"), hash_value(0xCD))];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].wasm_hash, "cd".repeat(32));
}

#[test]
fn the_pre_image_half_of_an_update_is_ignored() {
    // `state` carries the value BEFORE the change. Taking it would write the
    // old hash after the new one whenever both land in the same batch.
    let changes = vec![
        change("state", tag_key("fleet-v2"), hash_value(0x11)),
        change("updated", tag_key("fleet-v2"), hash_value(0x22)),
    ];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].wasm_hash, "22".repeat(32));
}

/// Nothing stops an owner re-pointing the same tag twice in one ledger. The
/// storage row is versioned by ledger, so both writes would carry the SAME
/// version and the merge would keep whichever it felt like — a fleet silently
/// running the wrong code half the time. Only the last one may survive.
#[test]
fn two_writes_to_one_tag_in_one_ledger_collapse_to_the_last() {
    let changes = vec![
        change("created", tag_key("fleet-v2"), hash_value(0x11)),
        change("updated", tag_key("fleet-v2"), hash_value(0x22)),
        change("updated", tag_key("fleet-v2"), hash_value(0x33)),
    ];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 1, "one row per (owner, tag) per ledger");
    assert_eq!(
        refs[0].wasm_hash,
        "33".repeat(32),
        "the survivor is the value the ledger ended on, not an arbitrary one"
    );
}

/// Different tags of the same owner are independent — the key is the pair, the
/// way an asset code means nothing without its issuer.
#[test]
fn different_tags_of_one_owner_do_not_collapse() {
    let changes = vec![
        change("created", tag_key("fleet-v1"), hash_value(0x11)),
        change("created", tag_key("fleet-v2"), hash_value(0x22)),
    ];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].tag, "fleet-v1");
    assert_eq!(refs[1].tag, "fleet-v2");
}

/// Across ledgers both rows stand: they are two facts, and the merge orders
/// them by version correctly.
#[test]
fn the_same_tag_in_two_ledgers_keeps_both_rows() {
    let mut later = change("updated", tag_key("fleet-v2"), hash_value(0x22));
    later.ledger_sequence += 1;
    let changes = vec![
        change("created", tag_key("fleet-v2"), hash_value(0x11)),
        later,
    ];

    let refs = extract_executable_ref_targets(&changes);

    assert_eq!(refs.len(), 2);
    assert_ne!(refs[0].ledger_sequence, refs[1].ledger_sequence);
}

#[test]
fn ordinary_contract_data_is_not_a_mapping() {
    let ordinary = scval_to_typed_json(&ScVal::Symbol(
        stellar_xdr::ScSymbol::try_from(b"Balance".to_vec()).unwrap(),
    ));

    let changes = vec![change("created", ordinary, hash_value(0xAB))];

    assert!(extract_executable_ref_targets(&changes).is_empty());
}

#[test]
fn a_value_that_is_not_32_bytes_is_not_a_hash() {
    // The protocol will not let this be written, so seeing one means we have
    // misread the entry — dropping it beats inventing a padded hash.
    let short = scval_to_typed_json(&ScVal::Bytes(ScBytes::try_from(vec![0xAB; 8]).unwrap()));

    let changes = vec![change("created", tag_key("fleet-v2"), short)];

    assert!(extract_executable_ref_targets(&changes).is_empty());
}

#[test]
fn reads_the_reference_off_an_instance() {
    let instance = ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::ExternalRef(ContractExecutableExternalRef {
            executable_owner: ScAddress::Contract(ContractId(Hash([0x11; 32]))),
            tag: ScString::try_from(b"fleet-v2".to_vec()).unwrap(),
        }),
        storage: None,
    });
    let data = json!({ "val": scval_to_typed_json(&instance) });

    let got = external_ref_from_instance(&data).expect("an external ref");

    assert!(got.owner.starts_with('C'));
    assert_eq!(got.tag, "fleet-v2");
}

#[test]
fn a_plain_wasm_instance_carries_no_reference() {
    let instance = ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::Wasm(Hash([0xAA; 32])),
        storage: None,
    });
    let data = json!({ "val": scval_to_typed_json(&instance) });

    assert_eq!(external_ref_from_instance(&data), None);
}
