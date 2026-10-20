//! Protocol 30 decode gates — written before the vote, like
//! `protocol_28_arms.rs`.
//!
//! Two of the three protocol-30 CAPs touch what we decode:
//!
//! - CAP-0088 — `STELLAR_VALUE_SIGNED_MS` / `STELLAR_VALUE_EMPTY_TX_SET_MS` on
//!   `StellarValue.ext`, carrying the close time in milliseconds. Every ledger
//!   after the upgrade ledger uses one of them, so without these arms the
//!   first protocol-30 ledger does not decode at all.
//! - CAP-0084 — `SC_ADDRESS_TYPE_MUXED_CONTRACT`, a contract address with a
//!   64-bit sub-account id, written as a `W…` strkey.
//!
//! No protocol-30 ledger exists yet, so the CAP-0088 test puts the new arm
//! into a real protocol-28 testnet ledger and decodes the whole ledger back.

use base64::Engine;
use stellar_xdr::{
    ContractId, Hash, LedgerCloseMeta, LedgerCloseValueSignature, Limits, MuxedContract, NodeId,
    PublicKey, ReadXdr, ScAddress, ScVal, Signature, StellarValueExt, StellarValueProposedMsValue,
    StellarValueSignedMsValue, TimePointMs, Uint256, WriteXdr,
};
use xdr_parser::ledger::extract_ledger;
use xdr_parser::scval::scval_to_typed_json;

const TESTNET_P28_LEDGER: &str = include_str!("fixtures/testnet_p28_ledger_4601991.b64");

fn testnet_ledger() -> LedgerCloseMeta {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(TESTNET_P28_LEDGER.trim())
        .expect("fixture is valid base64");
    LedgerCloseMeta::from_xdr(&bytes, Limits::none()).expect("fixture decodes")
}

#[test]
fn cap_88_millisecond_arm_decodes_and_close_time_stays_in_seconds() {
    let mut meta = testnet_ledger();
    let LedgerCloseMeta::V2(v2) = &mut meta else {
        panic!("the fixture closes as LedgerCloseMeta V2");
    };
    let close_time = v2.ledger_header.header.scp_value.close_time.0;
    v2.ledger_header.header.scp_value.ext = StellarValueExt::SignedMs(StellarValueSignedMsValue {
        close_time_ms: TimePointMs(close_time * 1000 + 250),
        lc_value_signature: LedgerCloseValueSignature {
            node_id: NodeId(PublicKey::PublicKeyTypeEd25519(Uint256([0x01; 32]))),
            signature: Signature(vec![0x02; 64].try_into().expect("64-byte signature")),
        },
    });

    let bytes = meta.to_xdr(Limits::none()).expect("encodes");
    let decoded = LedgerCloseMeta::from_xdr(&bytes, Limits::none())
        .expect("a ledger with the millisecond arm decodes");
    let extracted = extract_ledger(&decoded);

    assert_eq!(extracted.sequence, 4_601_991);
    assert_eq!(
        extracted.closed_at, close_time as i64,
        "`closed_at` stays whole seconds: CAP-0088 keeps `closeTime == closeTimeMs / 1000`"
    );
}

#[test]
fn cap_88_empty_tx_set_millisecond_arm_decodes_too() {
    // The other arm CAP-0088 adds: a ledger closed with an empty transaction
    // set carries the proposed value, now with the close time in ms.
    let mut meta = testnet_ledger();
    let LedgerCloseMeta::V2(v2) = &mut meta else {
        panic!("the fixture closes as LedgerCloseMeta V2");
    };
    let close_time = v2.ledger_header.header.scp_value.close_time.0;
    v2.ledger_header.header.scp_value.ext =
        StellarValueExt::EmptyTxSetMs(StellarValueProposedMsValue {
            close_time_ms: TimePointMs(close_time * 1000 + 750),
            tx_set_hash: Hash([0x00; 32]),
            previous_ledger_hash: Hash([0xAB; 32]),
            previous_ledger_version: 29,
            lc_value_signature: LedgerCloseValueSignature {
                node_id: NodeId(PublicKey::PublicKeyTypeEd25519(Uint256([0x01; 32]))),
                signature: Signature(vec![0x02; 64].try_into().expect("64-byte signature")),
            },
        });

    let bytes = meta.to_xdr(Limits::none()).expect("encodes");
    let decoded = LedgerCloseMeta::from_xdr(&bytes, Limits::none())
        .expect("a ledger with the empty-tx-set millisecond arm decodes");

    assert_eq!(extract_ledger(&decoded).closed_at, close_time as i64);
}

#[test]
fn cap_84_muxed_contract_renders_as_a_w_strkey() {
    let address = ScAddress::MuxedContract(MuxedContract {
        id: 12345,
        contract_id: ContractId(Hash([0xAB; 32])),
    });

    let rendered = scval_to_typed_json(&ScVal::Address(address));
    let value = rendered["value"].as_str().unwrap();

    assert_eq!(rendered["type"], "address");
    assert!(value.starts_with('W'), "muxed contract strkey: {value}");
    assert_eq!(value.len(), 69, "32-byte contract id + 8-byte id: {value}");
}
