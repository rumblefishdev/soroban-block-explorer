//! Protocol 28 ("Adapter") decode gates — written BEFORE the 2026-09-16 pubnet
//! vote, not after it.
//!
//! Protocol 27 arrived the other way round: the indexer linked an XDR version
//! that predated the vote, the first new-protocol ledger failed to parse, and
//! because a whole batch fails together the doorbell redelivered until it dead
//! lettered (~7.5k messages). These tests are the check that would have caught
//! it a week early.
//!
//! Two of the three protocol-28 CAPs touch the wire format, and a third change
//! rides along that neither CAP write-up mentions:
//!
//! - CAP-83 — `STELLAR_VALUE_EMPTY_TX_SET` on `StellarValue.ext`.
//! - CAP-85 — `CONTRACT_EXECUTABLE_EXTERNAL_REF` on `ContractExecutable`.
//! - unattributed — `SCV_EXECUTABLE_TAG` on `ScVal`, found by diffing the `.x`
//!   sources between `stellar-xdr` v27.0 and v28.0.

use base64::Engine;
use stellar_xdr::{
    ContractExecutable, ContractExecutableExternalRef, ContractId, Hash, LedgerCloseMeta,
    LedgerCloseValueSignature, Limits, NodeId, PublicKey, ReadXdr, ScAddress, ScContractInstance,
    ScString, ScVal, Signature, StellarValue, StellarValueExt, StellarValueProposedValue,
    TimePoint, Uint256, WriteXdr,
};
use xdr_parser::ledger::extract_ledger;
use xdr_parser::scval::scval_to_typed_json;

/// Ledger 4,601,991 of **testnet**, which voted to protocol 28 on 2026-08-27 —
/// three weeks ahead of pubnet. Fetched from `soroban-testnet.stellar.org`
/// (`getLedgers` → `metadataXdr`) on 2026-09-10, so this is real post-vote chain
/// data, not a hand-built value that only proves our own assumptions.
const TESTNET_P28_LEDGER: &str = include_str!("fixtures/testnet_p28_ledger_4601991.b64");

fn testnet_ledger() -> LedgerCloseMeta {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(TESTNET_P28_LEDGER.trim())
        .expect("fixture is valid base64");
    LedgerCloseMeta::from_xdr(&bytes, Limits::none())
        .expect("a real protocol-28 ledger must decode — this is the 0368 failure mode")
}

#[test]
fn a_real_protocol_28_ledger_decodes_and_reports_version_28() {
    let extracted = extract_ledger(&testnet_ledger());

    assert_eq!(extracted.sequence, 4_601_991);
    assert_eq!(
        extracted.protocol_version, 28,
        "the fixture is only worth keeping while it actually exercises protocol 28"
    );
    assert!(
        extracted.closed_at > 1_700_000_000,
        "close time comes from `scp_value.close_time`, the one field CAP-83 sits next to"
    );
}

#[test]
fn the_container_version_did_not_move_in_protocol_28() {
    // CAP-83 adds an arm INSIDE the header's consensus value; it does not bump
    // `LedgerCloseMeta` itself. If a future protocol adds a V3, the ingest path
    // needs a new arm in `ledger_header_entry` and this test is where that
    // shows up first.
    assert!(
        matches!(testnet_ledger(), LedgerCloseMeta::V2(_)),
        "protocol 28 still closes ledgers as LedgerCloseMeta V2"
    );
}

#[test]
fn cap_83_empty_tx_set_round_trips_and_close_time_still_reads() {
    // The arm that would have failed the decode. We read nothing from `ext`,
    // so the only thing that matters is that its presence does not stop the
    // rest of the header from being read.
    let value = StellarValue {
        tx_set_hash: Hash([0x00; 32]),
        close_time: TimePoint(1_789_000_000),
        upgrades: Vec::new().try_into().expect("no upgrades"),
        ext: StellarValueExt::EmptyTxSet(StellarValueProposedValue {
            tx_set_hash: Hash([0x00; 32]),
            previous_ledger_hash: Hash([0xAB; 32]),
            previous_ledger_version: 27,
            lc_value_signature: LedgerCloseValueSignature {
                node_id: NodeId(PublicKey::PublicKeyTypeEd25519(Uint256([0x01; 32]))),
                signature: Signature(vec![0x02; 64].try_into().expect("64-byte signature")),
            },
        }),
    };

    let bytes = value.to_xdr(Limits::none()).expect("encodes");
    let decoded = StellarValue::from_xdr(&bytes, Limits::none())
        .expect("stellar-xdr 27 cannot decode this arm; 28 must");

    assert!(matches!(decoded.ext, StellarValueExt::EmptyTxSet(_)));
    assert_eq!(decoded.close_time.0, 1_789_000_000);
}

#[test]
fn cap_85_external_ref_is_named_as_itself_and_never_as_wasm() {
    let instance = ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::ExternalRef(ContractExecutableExternalRef {
            executable_owner: ScAddress::Contract(ContractId(Hash([0x11; 32]))),
            tag: ScString::try_from(b"fleet-v2".to_vec()).expect("valid ScString"),
        }),
        storage: None,
    });

    let rendered = scval_to_typed_json(&instance);
    let executable = &rendered["value"]["executable"];

    assert_eq!(rendered["type"], "contract_instance");
    assert_eq!(executable["type"], "external_ref");
    assert_eq!(executable["tag"], "fleet-v2");
    assert!(
        executable["owner"].as_str().unwrap().starts_with('C'),
        "the owner is a contract, so its StrKey starts with C"
    );
    assert!(
        executable.get("hash").is_none(),
        "a fleet member runs the owner's code and has no hash of its own — \
         showing one would make it indistinguishable from a contract that does"
    );
}

#[test]
fn the_unattributed_executable_tag_scval_renders_as_a_string() {
    // `SCV_EXECUTABLE_TAG = 22`. Not in the CAP-85 write-up, but it lands on the
    // `ScVal` match rather than the `ContractExecutable` one, which makes it a
    // separate break site — and a separate chance to render nothing.
    let rendered = scval_to_typed_json(&ScVal::ExecutableTag(
        ScString::try_from(b"fleet-v2".to_vec()).expect("valid ScString"),
    ));

    assert_eq!(rendered["type"], "executable_tag");
    assert_eq!(rendered["value"], "fleet-v2");
}
