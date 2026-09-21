//! Claimable balances as holdings (task 0210).
//!
//! A `ClaimableBalanceEntry` holds value that has left its sender and does not
//! belong to any claimant yet, so no account or trustline carries it — but it
//! is part of the asset's supply. This module reads the entry's own state
//! changes, one holding per balance, the way `extract_soroban_token_balances`
//! reads `Balance(Address)` entries. It never sums transfer edges: an edge the
//! decoder drops would stay missing forever, while an entry row is exact and a
//! checkpoint snapshot can correct it.
//!
//! The entry is immutable apart from its sponsorship: created with its full
//! amount, removed whole when claimed or clawed back.

use std::collections::HashMap;

use serde_json::Value;
use stellar_xdr::{ClaimableBalanceId, Hash, ScAddress};
use tracing::error;

use crate::types::ExtractedLedgerEntryChange;

/// The asset a claimable balance holds. Classic only — a claimable balance
/// cannot carry a pool share or a contract token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimableBalanceAsset {
    Native,
    Credit { code: String, issuer: String },
}

/// One claimable balance as of a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedClaimableBalance {
    /// `B…` StrKey — the same rendering `asset_transfers` endpoints use, so the
    /// two can be reconciled per balance.
    pub balance_id: String,
    pub asset: ClaimableBalanceAsset,
    /// Stroops; 0 once the balance is gone.
    pub amount: i64,
    pub ledger_sequence: u32,
    /// The entry was removed (claimed or clawed back). ADR 0055.
    pub closed: bool,
}

/// Every claimable-balance holding one transaction's changes leave behind.
///
/// A `removed` change carries only the key, and the key has no asset, yet the
/// tombstone must land on the same `(holder, asset)` row as the live one or it
/// replaces nothing. The asset comes from the `state` pre-image the protocol
/// emits ahead of every removal, or from a create earlier in the same
/// transaction. A removal with neither is logged and dropped — writing it under
/// a guessed asset would close nothing and look like a real row.
pub fn extract_claimable_balances(
    changes: &[ExtractedLedgerEntryChange],
) -> Vec<ExtractedClaimableBalance> {
    let mut asset_by_id: HashMap<&str, ClaimableBalanceAsset> = HashMap::new();
    let mut out = Vec::new();

    for change in changes {
        if change.entry_type != "claimable_balance" {
            continue;
        }
        let Some(id) = change.key.get("balance_id").and_then(Value::as_str) else {
            continue;
        };
        let (asset, amount, closed) = match change.change_type.as_str() {
            // The pre-image only names the asset; its amount is the value BEFORE
            // this change and must not be written.
            "state" => {
                if let Some(asset) = change.data.as_ref().and_then(asset_of) {
                    asset_by_id.insert(id, asset);
                }
                continue;
            }
            "created" | "updated" | "restored" => {
                let data = change.data.as_ref();
                let asset = data.and_then(asset_of);
                let amount = data.and_then(|d| d.get("amount")).and_then(Value::as_i64);
                let (Some(asset), Some(amount)) = (asset, amount) else {
                    error!(
                        balance_id = id,
                        ledger = change.ledger_sequence,
                        "claimable balance entry without asset or amount — holding dropped"
                    );
                    continue;
                };
                asset_by_id.insert(id, asset.clone());
                (asset, amount, false)
            }
            "removed" => {
                let Some(asset) = asset_by_id.get(id).cloned() else {
                    error!(
                        balance_id = id,
                        ledger = change.ledger_sequence,
                        "claimable balance removed without a pre-image — tombstone dropped"
                    );
                    continue;
                };
                (asset, 0, true)
            }
            _ => continue,
        };
        let Some(balance_id) = balance_strkey(id) else {
            error!(
                balance_id = id,
                "claimable balance id is not 32 hex bytes — holding dropped"
            );
            continue;
        };
        out.push(ExtractedClaimableBalance {
            balance_id,
            asset,
            amount,
            ledger_sequence: change.ledger_sequence,
            closed,
        });
    }

    crate::fold::keep_last_by_key(out, |b| (b.balance_id.clone(), b.ledger_sequence))
}

fn asset_of(data: &Value) -> Option<ClaimableBalanceAsset> {
    let asset = data.get("asset")?;
    if asset.as_str() == Some("native") {
        return Some(ClaimableBalanceAsset::Native);
    }
    Some(ClaimableBalanceAsset::Credit {
        code: asset.get("code")?.as_str()?.to_string(),
        issuer: asset.get("issuer")?.as_str()?.to_string(),
    })
}

/// Hex balance id (as `ledger_entry_changes` renders it) → `B…` StrKey.
fn balance_strkey(hex_id: &str) -> Option<String> {
    let bytes: [u8; 32] = hex::decode(hex_id).ok()?.try_into().ok()?;
    Some(
        ScAddress::ClaimableBalance(ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(bytes)))
            .to_string(),
    )
}

#[cfg(test)]
#[path = "claimable_balance_tests.rs"]
mod tests;
