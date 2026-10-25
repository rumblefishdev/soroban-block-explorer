//! Staging for the value-flow tables (task 0540): `asset_transfers`,
//! `transaction_memos`.
//!
//! The parser has already decided *what* moved and between whom
//! (`xdr_parser::extract_asset_transfers`: emitter gate, payload shapes,
//! official identity, each end as a plain `G…` / `C…` with its sub-account
//! id). This module only turns StrKeys into surrogates and attaches the
//! transaction's `application_order` (its position in the ledger).
//!
//! Lives in its own file because `stage.rs` is past the module size limit;
//! `prepare_with_sac_overrides` calls [`build_value_flow_rows`] once per ledger.

use std::collections::HashMap;

use xdr_parser::types::ExtractedTransaction;
use xdr_parser::{ExtractedAssetTransfer, TokenEventKind};

use super::ids;
use super::rows::{AssetTransferRow, TransactionMemoRow};
use super::stage::event_asset_surrogate;
use crate::SchemaError;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ValueFlowRows {
    pub transfers: Vec<AssetTransferRow>,
    pub memos: Vec<TransactionMemoRow>,
}

pub fn build_value_flow_rows(
    ledger_sequence: i64,
    transactions: &[ExtractedTransaction],
    transfers: &[ExtractedAssetTransfer],
) -> Result<ValueFlowRows, SchemaError> {
    let mut out = ValueFlowRows::default();

    let mut order_by_hash: HashMap<&str, i16> = HashMap::with_capacity(transactions.len());
    for (idx, tx) in transactions.iter().enumerate() {
        let application_order = i16::try_from(idx + 1)
            .map_err(|_| SchemaError::Staging("application_order overflow (>i16)".into()))?;
        order_by_hash.insert(tx.hash.as_str(), application_order);
        if let (Some(memo_type), Some(memo)) = (&tx.memo_type, &tx.memo) {
            out.memos.push(TransactionMemoRow {
                ledger_sequence,
                application_order,
                memo_type: memo_type.clone(),
                memo: memo.clone(),
            });
        }
    }

    for t in transfers {
        let Some(&application_order) = order_by_hash.get(t.transaction_hash.as_str()) else {
            return Err(SchemaError::Staging(format!(
                "asset transfer for unknown transaction {}",
                t.transaction_hash
            )));
        };
        let (from_id, from_kind) = endpoint(t.from.as_deref());
        let (to_id, to_kind) = endpoint(t.to.as_deref());

        let emitter_id = ids::contract_id(&t.emitter);
        let asset_id = event_asset_surrogate(&t.asset, Some(emitter_id)).ok_or_else(|| {
            SchemaError::Staging(format!(
                "asset transfer without a resolvable asset (tx {}, op {}, event {})",
                t.transaction_hash, t.op_index, t.event_pos_in_op
            ))
        })?;

        out.transfers.push(AssetTransferRow {
            ledger_sequence,
            application_order,
            op_index: narrow(t.op_index, "op_index")?,
            event_pos_in_op: narrow(t.event_pos_in_op, "event_pos_in_op")?,
            asset_id,
            amount: t.amount,
            from_id,
            from_kind,
            from_muxed_id: t.from_muxed_id,
            to_id,
            to_kind,
            to_muxed_id: t.to_muxed_id,
            verb: verb(t.kind).to_string(),
        });
    }

    Ok(out)
}

/// Resolve one end of a transfer: `(surrogate, kind)`.
///
/// `kind` is the StrKey's first letter — `G` account, `C` contract, `L` classic
/// pool, `B` claimable balance — which tells the reader which table resolves
/// the id.
fn endpoint(strkey: Option<&str>) -> (Option<i64>, String) {
    let Some(s) = strkey else {
        return (None, String::new());
    };
    let kind = s.chars().next().map(|c| c.to_string()).unwrap_or_default();
    (Some(ids::address_id(s)), kind)
}

fn verb(kind: TokenEventKind) -> &'static str {
    match kind {
        TokenEventKind::Transfer => "transfer",
        TokenEventKind::Mint => "mint",
        TokenEventKind::Burn => "burn",
        TokenEventKind::Clawback => "clawback",
    }
}

fn narrow(v: u32, what: &str) -> Result<i16, SchemaError> {
    i16::try_from(v).map_err(|_| SchemaError::Staging(format!("{what} overflow (>i16): {v}")))
}

#[cfg(test)]
mod tests;
