//! Net-settled value per (transaction, asset) — task 0393.
//!
//! A Stellar transaction has no protocol-level "amount"; value lives on its
//! operations and its Soroban token events. To surface a single "value moved"
//! figure per (transaction, asset) for the tx-list views, we reduce every value
//! movement of one transaction to the **net settled value**:
//!
//! ```text
//! delta[account] += amount   for the `to` of each movement
//! delta[account] -= amount   for the `from` of each movement
//! amount = max( Σ positive deltas , Σ negative deltas )
//! ```
//!
//! `max` of *both* sides (not just the gained side) keeps burns and
//! payments-to-issuer — where nobody gains — non-zero. Netting per account
//! drops routing hops automatically (a pass-through account has delta 0), so it
//! avoids the gross double-count. See the task's `S-formula-and-edge-cases`
//! note for the derivation and the alternatives rejected.
//!
//! This reducer is pure and works on resolved **surrogate ids** (`i64` asset
//! and account). Two rules from the formula are the caller's responsibility,
//! because they are resolution concerns, not arithmetic:
//!
//! - **Native XLM canonicalised to one `asset_id`** — the caller passes the
//!   single native surrogate (`hash64("native")`) for both native conventions
//!   so the deltas cancel.
//! - **`fee` events excluded** — the caller drops fee movements before calling.

use std::collections::BTreeMap;

/// One signed value movement within a single transaction, resolved to
/// surrogate ids.
///
/// `from`/`to` are account surrogates. `None` marks a one-sided event:
/// `from == None` is a **mint** (value created), `to == None` is a
/// **burn/clawback** (value destroyed). `amount` is the raw, unscaled token
/// quantity (non-negative; decimals are applied at read time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movement<A> {
    pub asset_id: i64,
    pub from: Option<A>,
    pub to: Option<A>,
    pub amount: i128,
}

/// Net-settled value for one (transaction, asset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSettled {
    pub asset_id: i64,
    /// `max(Σ positive deltas, Σ negative deltas)`, raw (unscaled).
    pub amount: i128,
}

/// Reduce a transaction's value movements to the net-settled value per asset.
///
/// Returns one [`NetSettled`] per distinct `asset_id`, ordered by `asset_id`
/// for deterministic output. An empty input yields an empty vec.
pub fn net_settled<A: Ord>(movements: &[Movement<A>]) -> Vec<NetSettled> {
    // asset_id -> (account -> signed delta)
    let mut per_asset: BTreeMap<i64, BTreeMap<&A, i128>> = BTreeMap::new();
    for m in movements {
        let deltas = per_asset.entry(m.asset_id).or_default();
        if let Some(to) = &m.to {
            *deltas.entry(to).or_default() += m.amount;
        }
        if let Some(from) = &m.from {
            *deltas.entry(from).or_default() -= m.amount;
        }
    }

    per_asset
        .into_iter()
        .map(|(asset_id, deltas)| {
            let gained: i128 = deltas.values().filter(|d| **d > 0).sum();
            let lost: i128 = deltas.values().filter(|d| **d < 0).map(|d| -d).sum();
            NetSettled {
                asset_id,
                amount: gained.max(lost),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const X: i64 = 100; // asset X surrogate
    const Y: i64 = 200; // asset Y surrogate
    const A: i64 = 1;
    const B: i64 = 2;
    const C: i64 = 3;
    const D: i64 = 4;
    const ISSUER: i64 = 9;

    fn transfer(asset: i64, from: i64, to: i64, amount: i128) -> Movement<i64> {
        Movement {
            asset_id: asset,
            from: Some(from),
            to: Some(to),
            amount,
        }
    }
    fn mint(asset: i64, to: i64, amount: i128) -> Movement<i64> {
        Movement {
            asset_id: asset,
            from: None,
            to: Some(to),
            amount,
        }
    }
    fn burn(asset: i64, from: i64, amount: i128) -> Movement<i64> {
        Movement {
            asset_id: asset,
            from: Some(from),
            to: None,
            amount,
        }
    }
    fn one(r: &[NetSettled]) -> &NetSettled {
        assert_eq!(r.len(), 1, "expected one asset row, got {r:?}");
        &r[0]
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(net_settled::<i64>(&[]), vec![]);
    }

    #[test]
    fn plain_transfer_is_its_amount() {
        let r = net_settled(&[transfer(X, A, B, 100)]);
        let n = one(&r);
        assert_eq!((n.asset_id, n.amount), (X, 100));
    }

    #[test]
    fn routing_chain_nets_to_moved_value_not_gross() {
        // A -> B -> C, 100. Gross would be 200; net is 100 (B passes through).
        let r = net_settled(&[transfer(X, A, B, 100), transfer(X, B, C, 100)]);
        assert_eq!(one(&r).amount, 100);
    }

    #[test]
    fn pure_mint_counts_the_created_value() {
        let n = &net_settled(&[mint(X, A, 100)])[0];
        assert_eq!(n.amount, 100);
    }

    #[test]
    fn pure_burn_counts_the_destroyed_value() {
        // Σ+ alone would give 0 here; max(Σ+, Σ−) keeps it at 100.
        let n = &net_settled(&[burn(X, A, 100)])[0];
        assert_eq!(n.amount, 100);
    }

    #[test]
    fn transfer_plus_burn_takes_the_larger_side() {
        // transfer 100 (A->B) + burn 40 (B). Σ+ = 60, Σ− = 100 -> 100.
        let r = net_settled(&[transfer(X, A, B, 100), burn(X, B, 40)]);
        assert_eq!(one(&r).amount, 100);
    }

    #[test]
    fn redeem_to_issuer_as_two_sided_transfer() {
        // redeem 250 recorded as a transfer to the issuer.
        let n = &net_settled(&[transfer(X, A, ISSUER, 250)])[0];
        assert_eq!(n.amount, 250);
    }

    #[test]
    fn redeem_one_sided_is_representation_robust() {
        // Same redeem recorded one-sided (burn-shaped) still nets to 250.
        let n = &net_settled(&[burn(X, A, 250)])[0];
        assert_eq!(n.amount, 250);
    }

    #[test]
    fn self_cancelling_mint_transfer_burn_nets_to_zero() {
        // mint 100 -> A, A -> B 100, burn 100 from B. Every account nets to 0.
        let r = net_settled(&[mint(X, A, 100), transfer(X, A, B, 100), burn(X, B, 100)]);
        assert_eq!(one(&r).amount, 0);
    }

    #[test]
    fn two_assets_split_into_two_rows_ordered_by_asset_id() {
        // asset X transfer 100, asset Y transfer 50, in one transaction.
        let r = net_settled(&[transfer(Y, B, A, 50), transfer(X, A, B, 100)]);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].asset_id, r[0].amount), (X, 100)); // ordered by asset_id
        assert_eq!((r[1].asset_id, r[1].amount), (Y, 50));
    }

    #[test]
    fn independent_transfers_of_same_asset_sum() {
        // A->B 100 and C->D 30 share no account: net is 130, not 100.
        let r = net_settled(&[transfer(X, A, B, 100), transfer(X, C, D, 30)]);
        assert_eq!(one(&r).amount, 130);
    }
}
