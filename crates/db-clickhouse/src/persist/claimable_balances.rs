//! Staging for `claimable_balance_holdings` (task 0210).
//!
//! The parser has already turned `ClaimableBalanceEntry` changes into holdings
//! (`xdr_parser::claimable_balance`). This module maps them onto the `balances`
//! row shape — the table is `balances`' twin, kept apart only because its churn
//! and its readers differ (ADR 0056 amendment 2026-09-15) — and folds them
//! across the whole ledger.
//!
//! Lives in its own file because `stage.rs` is past the module size limit;
//! `prepare_with_sac_overrides` calls [`build_claimable_balance_rows`] once per
//! ledger.

use xdr_parser::claimable_balance::{ClaimableBalanceAsset, ExtractedClaimableBalance};

use super::ids;
use super::rows::BalanceRow;

/// One row per `(balance, ledger)`.
///
/// The parser folds each transaction on its own, so a balance created in one
/// transaction and claimed in a later one of the same ledger arrives as two
/// holdings with the same version, and the ReplacingMergeTree would keep
/// whichever was inserted last. `balances` arrives in application order, so
/// the last holding per key is the state the ledger ended on (ADR 0057
/// decision 6).
pub fn build_claimable_balance_rows(balances: &[ExtractedClaimableBalance]) -> Vec<BalanceRow> {
    let rows = balances
        .iter()
        .map(|b| {
            let ledger = i64::from(b.ledger_sequence);
            BalanceRow {
                holder_id: ids::address_id(&b.balance_id),
                asset_id: match &b.asset {
                    ClaimableBalanceAsset::Native => ids::NATIVE_ASSET_ID,
                    ClaimableBalanceAsset::Credit { code, issuer } => {
                        ids::credit_asset_id(code, issuer)
                    }
                },
                // Stroops are the raw unit of every classic `balances` amount.
                amount: i128::from(b.amount),
                last_updated_ledger: ledger,
                closed_at_ledger: if b.closed { ledger } else { 0 },
            }
        })
        .collect();
    xdr_parser::fold::keep_last_by_key(rows, |r: &BalanceRow| {
        (r.holder_id, r.asset_id, r.last_updated_ledger)
    })
}

#[cfg(test)]
#[path = "claimable_balances_tests.rs"]
mod tests;
