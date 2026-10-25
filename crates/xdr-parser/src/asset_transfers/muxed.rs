//! The sub-account ids of a transfer's two ends (`from_muxed_id` /
//! `to_muxed_id`).
//!
//! CAP-67 keeps `to` as the underlying `G…` and puts the id in the event data
//! — merged with the memo in a classic operation, the recipient's own
//! sub-account id in a Soroban call (CAP-0084 adds the `W…` contract form). So
//! a Soroban transfer takes the event's id, and a classic one takes the
//! envelope's `M…` destination and source, matched to the transfer through
//! `op_index`. A topic that *is* an `M…` / `W…` is split here the same way, so
//! every transfer leaves the parser with a plain `G…` / `C…`.

use domain::OperationType;
use serde_json::Value;
use tracing::warn;

use crate::event_filters::EventAsset;
use crate::types::ExtractedOperation;

/// The sender's id: the op's own source if it overrides, else the tx source.
pub(super) fn sender_id(
    op: Option<&ExtractedOperation>,
    tx_source_account: &str,
    tx_source_muxed_id: Option<u64>,
    g: &str,
) -> Option<u64> {
    match op {
        Some(op) if op.source_account.is_some() => {
            (op.source_account.as_deref() == Some(g)).then_some(op.source_muxed_id)
        }
        _ => (tx_source_account == g).then_some(tx_source_muxed_id),
    }
    .flatten()
}

/// The recipient's id, when the topic itself is not an `M…` / `W…`.
pub(super) fn recipient_id(
    op: Option<&ExtractedOperation>,
    asset: &EventAsset,
    event_to_muxed_id: Option<u64>,
    g: &str,
) -> Option<u64> {
    // A Soroban call names the recipient's sub-account in the event
    // itself: the topic keeps the plain `G…` / `C…` and `to_muxed_id`
    // carries the id (CAP-67, CAP-0084). In a classic operation that
    // field may be the transaction memo instead, so the id comes from
    // the op's own destination below.
    if op.is_some_and(|op| op.op_type == OperationType::InvokeHostFunction) {
        return event_to_muxed_id;
    }
    // Recipient: the op's own destination — and only for the transfer
    // that delivers the op's asset to it. A path payment that crosses
    // the recipient's own offer moves other assets to the same address
    // inside the same op, and an arbitrage with source == destination
    // moves several; none of those is the exchange deposit the
    // sub-account id names (deep review, 2026-09-07).
    op.filter(|op| op.details.get("destination").and_then(Value::as_str) == Some(g))
        .filter(|op| op_delivers(op, asset))
        .and_then(|op| op.destination_muxed_id)
}

/// Does this operation deliver `asset` to its destination? Payment: `asset`;
/// path payments: `destAsset`; AccountMerge carries no asset in its details
/// and moves native. A bespoke token is never what a classic operation
/// delivers.
fn op_delivers(op: &ExtractedOperation, asset: &EventAsset) -> bool {
    let delivered = op
        .details
        .get("destAsset")
        .or_else(|| op.details.get("asset"))
        .and_then(Value::as_str);
    match (delivered, asset) {
        (None | Some("native"), EventAsset::Native) => true,
        (Some(s), EventAsset::Credit { code, issuer }) => {
            s.split_once(':') == Some((code.as_str(), issuer.as_str()))
        }
        _ => false,
    }
}

/// Resolve one end of a transfer: `(address, muxed id)`.
///
/// An `M…` is split into its `G…` and a `W…` into its `C…` (so the account or
/// contract page finds it) and its id; otherwise the id comes from
/// `id_outside_topic` — the event for a Soroban call, the envelope for a
/// classic operation.
pub(super) fn endpoint(
    strkey: Option<String>,
    id_outside_topic: impl FnOnce(&str) -> Option<u64>,
) -> (Option<String>, Option<u64>) {
    let Some(s) = strkey else {
        return (None, None);
    };
    let (g, topic_muxed_id) = split_muxed(s);
    let muxed_id = topic_muxed_id.or_else(|| id_outside_topic(&g));
    (Some(g), muxed_id)
}

/// `M…` → (`G…`, id), `W…` → (`C…`, id); anything else unchanged.
fn split_muxed(strkey: String) -> (String, Option<u64>) {
    if !strkey.starts_with('M') && !strkey.starts_with('W') {
        return (strkey, None);
    }
    match stellar_strkey::Strkey::from_string(&strkey) {
        Ok(stellar_strkey::Strkey::MuxedAccountEd25519(m)) => {
            let g = stellar_strkey::Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(
                m.ed25519,
            ));
            (g.to_string().as_str().to_owned(), Some(m.id))
        }
        Ok(stellar_strkey::Strkey::MuxedContract(m)) => {
            let c = stellar_strkey::Strkey::Contract(stellar_strkey::Contract(m.contract_id));
            (c.to_string().as_str().to_owned(), Some(m.id))
        }
        // The topic was rendered from an `ScAddress` by our own decoder, so a
        // malformed `M…` / `W…` here is a decoder bug: the row keeps the
        // address as it came, with no id, and the log names it.
        _ => {
            warn!(
                target: "xdr_parser::asset_transfers",
                %strkey,
                "transfer end starts like a muxed address but is not one — kept as is, no sub-account id"
            );
            (strkey, None)
        }
    }
}
