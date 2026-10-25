use super::*;
use domain::{ContractEventType, OperationType};
use serde_json::json;
use xdr_parser::types::{EventBody, ExtractedEvent, ExtractedOperation};
use xdr_parser::{EventId, EventOrigin};

const TX: &str = "0a120260ab2a4d3e7f9c1b5d6e8f0a1b2c3d4e5f60718293a4b5c6d7e8f9a0b1";
const G_SENDER: &str = "GARNRDKOUGVQ6FMJLL5RPDU6NG36LZHTEOIAWZPNOKFNKNKCOJEC3WMZ";
const G_RECEIVER: &str = "GD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2FTL";
/// `G_RECEIVER` + id 3539365402, built and round-tripped in the research note.
const M_RECEIVER: &str = "MD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2AAAAAANF5TODJEYA";
const POOL_L: &str = "LA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ";
const XLM_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
const C_RECEIVER: &str = "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA";

#[test]
fn sep50_token_number_does_not_become_a_persisted_amount() {
    const NFT_CONTRACT: &str = "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA";
    let mut ev = event(0, 0);
    ev.body.contract_id = Some(NFT_CONTRACT.into());
    // No asset label: this event is attributed to its own emitter, never USDC.
    ev.body.topics = json!([
        {"type":"sym", "value":"mint"},
        {"type":"address", "value":G_RECEIVER}
    ]);
    ev.body.data = xdr_parser::scval::scval_to_typed_json(&stellar_xdr::ScVal::U128(
        stellar_xdr::UInt128Parts {
            hi: 0,
            lo: 1_000_000_000,
        },
    ));
    let net = xdr_parser::network_id(xdr_parser::sac::MAINNET_PASSPHRASE);
    let decoded = xdr_parser::extract_asset_transfers(&[ev], &[], G_SENDER, None, &net);
    assert!(decoded.rejects.is_empty());
    let out = build_value_flow_rows(64_259_660, &[tx(None, None)], &decoded.transfers).unwrap();
    assert_eq!(out.transfers.len(), 1);
    assert_eq!(out.transfers[0].amount, None);
    assert_eq!(out.transfers[0].to_id, Some(ids::account_id(G_RECEIVER)));
    assert_eq!(out.transfers[0].asset_id, ids::contract_id(NFT_CONTRACT));
}

fn tx(memo: Option<(&str, &str)>, source_muxed_id: Option<u64>) -> ExtractedTransaction {
    ExtractedTransaction {
        hash: TX.into(),
        inner_tx_hash: None,
        ledger_sequence: 64_259_660,
        source_account: G_SENDER.into(),
        source_muxed_id,
        fee_source: None,
        fee_charged: 100,
        successful: true,
        result_code: "txSuccess".into(),
        envelope_xdr: String::new(),
        result_xdr: String::new(),
        result_meta_xdr: None,
        operation_tree: None,
        memo_type: memo.map(|m| m.0.to_string()),
        memo: memo.map(|m| m.1.to_string()),
        created_at: 0,
        parse_error: false,
        ledger_deltas: Vec::new(),
    }
}

fn payment_op(
    index: u32,
    destination: &str,
    destination_muxed_id: Option<u64>,
    source: Option<(&str, Option<u64>)>,
) -> ExtractedOperation {
    ExtractedOperation {
        transaction_hash: TX.into(),
        operation_index: index,
        op_type: OperationType::Payment,
        source_account: source.map(|s| s.0.to_string()),
        details: json!({ "destination": destination, "asset": "native", "amount": 10000 }),
        asset_appearances: Vec::new(),
        counterparties: Vec::new(),
        source_muxed_id: source.and_then(|s| s.1),
        destination_muxed_id,
    }
}

/// A native-XLM token event of 10 000 stroops, emitted by the XLM SAC at
/// position `pos` of operation `op_index`.
fn transfer(
    op_index: u32,
    pos: u32,
    from: Option<&str>,
    to: Option<&str>,
    kind: TokenEventKind,
) -> ExtractedEvent {
    let addr = |a: Option<&str>| json!({ "type": "address", "value": a.expect("operand") });
    let verb = |v: &str| json!({ "type": "sym", "value": v });
    let native = json!({ "type": "string", "value": "native" });
    let mut ev = event(u16::try_from(op_index).expect("op index"), pos);
    ev.body.topics = match kind {
        TokenEventKind::Transfer => json!([verb("transfer"), addr(from), addr(to), native]),
        TokenEventKind::Mint => json!([verb("mint"), addr(to), native]),
        TokenEventKind::Burn => json!([verb("burn"), addr(from), native]),
        TokenEventKind::Clawback => json!([verb("clawback"), addr(from), native]),
    };
    ev.body.data = json!({ "type": "i128", "value": "10000" });
    ev
}

/// The event's own `to_muxed_id` (CAP-67 `map{amount, to_muxed_id}`).
fn with_event_muxed_id(mut ev: ExtractedEvent, id: u64) -> ExtractedEvent {
    ev.body.data = json!({ "type": "map", "value": [
        { "key": { "type": "sym", "value": "amount" }, "value": { "type": "i128", "value": "10000" } },
        { "key": { "type": "sym", "value": "to_muxed_id" }, "value": { "type": "u64", "value": id } },
    ]});
    ev
}

/// The same movement of a credit asset, emitted by that asset's own SAC.
fn as_credit(mut ev: ExtractedEvent, code: &str, issuer: &str) -> ExtractedEvent {
    let net = xdr_parser::network_id(xdr_parser::sac::MAINNET_PASSPHRASE);
    ev.body.contract_id = xdr_parser::derive_sac_strkey(code, issuer, &net);
    let topics = ev.body.topics.as_array_mut().expect("topics");
    *topics.last_mut().expect("asset topic") =
        json!({ "type": "string", "value": format!("{code}:{issuer}") });
    ev
}

/// The decode the indexer runs on transaction `TX` — with its envelope source
/// and the operations of `ops` that belong to it.
fn decode(
    tx: &ExtractedTransaction,
    ops: &[(String, Vec<ExtractedOperation>)],
    events: &[ExtractedEvent],
) -> Vec<ExtractedAssetTransfer> {
    let tx_ops = ops
        .iter()
        .find(|(hash, _)| *hash == tx.hash)
        .map_or(&[][..], |(_, ops)| ops.as_slice());
    let net = xdr_parser::network_id(xdr_parser::sac::MAINNET_PASSPHRASE);
    let decoded = xdr_parser::extract_asset_transfers(
        events,
        tx_ops,
        &tx.source_account,
        tx.source_muxed_id,
        &net,
    );
    assert!(decoded.rejects.is_empty(), "{:?}", decoded.rejects);
    decoded.transfers
}

/// Decode `events` for the one transaction in `txs`, then stage the rows.
fn rows(
    txs: &[ExtractedTransaction],
    ops: &[(String, Vec<ExtractedOperation>)],
    events: &[ExtractedEvent],
) -> Result<ValueFlowRows, SchemaError> {
    build_value_flow_rows(64_259_660, txs, &decode(&txs[0], ops, events))
}

fn invoke_op(index: u32) -> ExtractedOperation {
    ExtractedOperation {
        transaction_hash: TX.into(),
        operation_index: index,
        op_type: OperationType::InvokeHostFunction,
        source_account: None,
        details: json!({}),
        asset_appearances: Vec::new(),
        counterparties: Vec::new(),
        source_muxed_id: None,
        destination_muxed_id: None,
    }
}

/// `C_RECEIVER` muxed with `id` (CAP-0084), as a `W…` strkey.
fn w_receiver(id: u64) -> String {
    let stellar_strkey::Strkey::Contract(c) =
        stellar_strkey::Strkey::from_string(C_RECEIVER).expect("valid contract strkey")
    else {
        panic!("C_RECEIVER is a contract strkey");
    };
    stellar_strkey::Strkey::MuxedContract(stellar_strkey::MuxedContract {
        contract_id: c.0,
        id,
    })
    .to_string()
    .as_str()
    .to_owned()
}

/// The event at position `pos` of operation `op`.
fn event(op: u16, pos: u32) -> ExtractedEvent {
    ExtractedEvent {
        transaction_hash: TX.into(),
        event_id: EventId {
            ledger_sequence: 64_259_660,
            transaction_index: 1,
            operation_index: op,
            event_index: pos,
        },
        origin: EventOrigin::Operation(op),
        body: EventBody {
            event_type: ContractEventType::Contract,
            contract_id: Some(XLM_SAC.into()),
            topics: json!([]),
            data: json!({ "type": "void" }),
        },
        created_at: 0,
    }
}

#[test]
fn plain_payment_row_carries_ids_kinds_and_the_official_identity() {
    let txs = [tx(None, None)];
    let ops = [(TX.to_string(), vec![payment_op(0, G_RECEIVER, None, None)])];
    let t = [transfer(
        0,
        0,
        Some(G_SENDER),
        Some(G_RECEIVER),
        TokenEventKind::Transfer,
    )];

    let out = rows(&txs, &ops, &t).unwrap();
    assert_eq!(
        out.transfers,
        vec![AssetTransferRow {
            ledger_sequence: 64_259_660,
            application_order: 1,
            op_index: 0,
            event_pos_in_op: 0,
            asset_id: ids::NATIVE_ASSET_ID,
            amount: Some(10_000),
            from_id: Some(ids::account_id(G_SENDER)),
            from_kind: "G".into(),
            from_muxed_id: None,
            to_id: Some(ids::account_id(G_RECEIVER)),
            to_kind: "G".into(),
            to_muxed_id: None,
            verb: "transfer".into(),
        }]
    );
    assert!(out.memos.is_empty());
}

#[test]
fn muxed_destination_comes_from_the_envelope_and_keeps_the_g_surrogate() {
    // The event says `to = G…` (CAP-67); the envelope's Payment says `M…`.
    let txs = [tx(None, None)];
    let ops = [(
        TX.to_string(),
        vec![payment_op(0, G_RECEIVER, Some(3_539_365_402), None)],
    )];
    let t = [transfer(
        0,
        0,
        Some(G_SENDER),
        Some(G_RECEIVER),
        TokenEventKind::Transfer,
    )];

    let row = &rows(&txs, &ops, &t).unwrap().transfers[0];
    assert_eq!(
        row.to_id,
        Some(ids::account_id(G_RECEIVER)),
        "hash of the G, never of the M"
    );
    assert_eq!(row.to_muxed_id, Some(3_539_365_402));
}

#[test]
fn muxed_id_is_not_borrowed_from_an_op_whose_destination_is_someone_else() {
    // Path payment: the op destination is G_RECEIVER (muxed), but this transfer
    // is an intermediate hop to a pool. The pool must not inherit the id.
    let txs = [tx(None, None)];
    let ops = [(
        TX.to_string(),
        vec![payment_op(0, G_RECEIVER, Some(42), None)],
    )];
    let t = [transfer(
        0,
        1,
        Some(G_SENDER),
        Some(POOL_L),
        TokenEventKind::Transfer,
    )];

    let row = &rows(&txs, &ops, &t).unwrap().transfers[0];
    assert_eq!(row.to_kind, "L");
    assert_eq!(row.to_muxed_id, None);
}

#[test]
fn muxed_source_comes_from_the_op_override_or_the_tx_source() {
    let txs = [tx(None, Some(7))];
    let ops = [(
        TX.to_string(),
        vec![
            payment_op(0, G_RECEIVER, None, None),
            payment_op(1, G_RECEIVER, None, Some((G_RECEIVER, Some(9)))),
        ],
    )];
    let t = [
        // op 0 inherits the tx source (muxed id 7)
        transfer(
            0,
            0,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
        // op 1 overrides the source with G_RECEIVER (muxed id 9); a transfer
        // FROM the tx source inside it must not get 7 or 9.
        transfer(
            1,
            0,
            Some(G_RECEIVER),
            Some(G_SENDER),
            TokenEventKind::Transfer,
        ),
        transfer(
            1,
            1,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
    ];

    let rows = rows(&txs, &ops, &t).unwrap().transfers;
    assert_eq!(rows[0].from_muxed_id, Some(7));
    assert_eq!(rows[1].from_muxed_id, Some(9));
    assert_eq!(rows[2].from_muxed_id, None);
}

#[test]
fn an_m_address_in_the_topic_itself_is_split() {
    let txs = [tx(None, None)];
    let t = [transfer(
        0,
        0,
        Some(G_SENDER),
        Some(M_RECEIVER),
        TokenEventKind::Transfer,
    )];
    let row = &rows(&txs, &[], &t).unwrap().transfers[0];
    assert_eq!(row.to_id, Some(ids::account_id(G_RECEIVER)));
    assert_eq!(row.to_kind, "G");
    assert_eq!(row.to_muxed_id, Some(3_539_365_402));
}

#[test]
fn mint_and_burn_leave_the_missing_end_null() {
    let txs = [tx(None, None)];
    let t = [
        transfer(0, 0, None, Some(G_RECEIVER), TokenEventKind::Mint),
        transfer(0, 1, Some(G_RECEIVER), None, TokenEventKind::Clawback),
    ];
    let rows = rows(&txs, &[], &t).unwrap().transfers;
    assert_eq!(
        (
            rows[0].from_id,
            rows[0].from_kind.as_str(),
            rows[0].verb.as_str()
        ),
        (None, "", "mint")
    );
    assert_eq!(
        (
            rows[1].to_id,
            rows[1].to_kind.as_str(),
            rows[1].verb.as_str()
        ),
        (None, "", "clawback")
    );
}

#[test]
fn memo_is_one_row_per_transaction_not_per_transfer() {
    let txs = [tx(Some(("text", "pspb:5721732")), None)];
    let t = [
        transfer(
            0,
            0,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
        transfer(
            0,
            1,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
    ];
    let out = rows(&txs, &[], &t).unwrap();
    assert_eq!(out.transfers.len(), 2);
    assert_eq!(
        out.memos,
        vec![TransactionMemoRow {
            ledger_sequence: 64_259_660,
            application_order: 1,
            memo_type: "text".into(),
            memo: "pspb:5721732".into(),
        }]
    );
}

#[test]
fn a_transfer_for_an_unknown_transaction_is_a_staging_error_not_a_dropped_row() {
    let t = [transfer(
        0,
        0,
        Some(G_SENDER),
        Some(G_RECEIVER),
        TokenEventKind::Transfer,
    )];
    let decoded = decode(&tx(None, None), &[], &t);
    let err = build_value_flow_rows(64_259_660, &[], &decoded).unwrap_err();
    assert!(matches!(err, SchemaError::Staging(_)));
}

// ---- deep-review fix J13 (2026-09-07): the sub-account id follows the asset

/// A path payment to a muxed recipient can also move ANOTHER asset to the
/// same `G…` inside the same operation (the recipient's own offer crossed on
/// the path). Only the transfer of the op's `destAsset` is the deposit the
/// sub-account id names; the other keeps `to_muxed_id = NULL`.
#[test]
fn muxed_id_follows_only_the_transfer_of_the_ops_delivered_asset() {
    const USDC_ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let txs = [tx(None, None)];
    let mut path = payment_op(0, G_RECEIVER, Some(3_539_365_402), None);
    path.op_type = OperationType::PathPaymentStrictSend;
    path.details = json!({
        "destination": G_RECEIVER,
        "sendAsset": "native",
        "destAsset": format!("USDC:{USDC_ISSUER}"),
    });
    let ops = [(TX.to_string(), vec![path])];

    let usdc = as_credit(
        transfer(
            0,
            0,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
        "USDC",
        USDC_ISSUER,
    );
    // The recipient's own XLM offer, crossed on the path: native to the same G.
    let xlm = transfer(
        0,
        1,
        Some(G_SENDER),
        Some(G_RECEIVER),
        TokenEventKind::Transfer,
    );

    let out = rows(&txs, &ops, &[usdc, xlm]).unwrap();
    assert_eq!(
        out.transfers[0].to_muxed_id,
        Some(3_539_365_402),
        "the delivered asset"
    );
    assert_eq!(
        out.transfers[1].to_muxed_id, None,
        "the crossed offer is not the deposit"
    );
}

#[test]
fn a_w_address_in_the_topic_is_split_into_its_contract_and_id() {
    let w = w_receiver(42);
    let txs = [tx(None, None)];
    let t = [transfer(
        0,
        0,
        Some(G_SENDER),
        Some(&w),
        TokenEventKind::Transfer,
    )];
    let row = &rows(&txs, &[], &t).unwrap().transfers[0];
    assert_eq!(row.to_id, Some(ids::contract_id(C_RECEIVER)));
    assert_eq!(row.to_kind, "C");
    assert_eq!(row.to_muxed_id, Some(42));
}

/// CAP-0084: a Soroban transfer to a `W…` keeps the plain `C…` in the topic
/// and carries the id in the event's `to_muxed_id`; CAP-67 does the same for
/// an `M…` recipient with a plain `G…` in the topic.
#[test]
fn a_soroban_transfer_takes_the_recipients_id_from_its_event() {
    let txs = [tx(None, None)];
    let ops = [(TX.to_string(), vec![invoke_op(0)])];
    let to_contract = with_event_muxed_id(
        transfer(
            0,
            0,
            Some(G_SENDER),
            Some(C_RECEIVER),
            TokenEventKind::Transfer,
        ),
        42,
    );
    let to_account = with_event_muxed_id(
        transfer(
            0,
            1,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
        3_539_365_402,
    );
    let out = rows(&txs, &ops, &[to_contract, to_account]).unwrap();
    assert_eq!(out.transfers[0].to_kind, "C");
    assert_eq!(out.transfers[0].to_id, Some(ids::contract_id(C_RECEIVER)));
    assert_eq!(out.transfers[0].to_muxed_id, Some(42));
    assert_eq!(out.transfers[1].to_kind, "G");
    assert_eq!(out.transfers[1].to_muxed_id, Some(3_539_365_402));
}

/// CAP-67 rule 2: in a classic operation `to_muxed_id` may be the
/// transaction's id memo, which `transaction_memos` already holds — it never
/// becomes a sub-account id.
#[test]
fn a_classic_transfer_ignores_the_event_id_which_may_be_the_memo() {
    let txs = [tx(Some(("id", "777")), None)];
    let ops = [(TX.to_string(), vec![payment_op(0, G_RECEIVER, None, None)])];
    let t = with_event_muxed_id(
        transfer(
            0,
            0,
            Some(G_SENDER),
            Some(G_RECEIVER),
            TokenEventKind::Transfer,
        ),
        777,
    );
    let row = &rows(&txs, &ops, &[t]).unwrap().transfers[0];
    assert_eq!(row.to_muxed_id, None);
}
