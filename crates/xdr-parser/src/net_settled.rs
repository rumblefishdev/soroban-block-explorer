//! Net-settled value per (transaction, asset) — task 0393.
//!
//! A Stellar transaction has no protocol-level "amount"; value moves as ledger
//! balance changes across its operations and Soroban invocations. To surface a
//! single "value moved" figure per (transaction, asset) for the tx-list views,
//! we reduce every value movement of one transaction to the **net settled
//! value**:
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
//! # Why this is the flow value, not a heuristic
//!
//! This figure is the **value of the flow** in the sense of the network-flow
//! *flow decomposition theorem*: every flow decomposes into source→sink **paths**
//! plus **cycles**, where a path contributes its flow to the value and a **cycle
//! contributes exactly zero**. `max(Σ+, Σ−)` is that value — the total leaving
//! the sources, equivalently the total reaching the sinks. So:
//!
//! ```text
//! gross = Σ path flows + Σ cycle flows
//! net   = Σ path flows                  <- this function; cycles are 0 BY THEOREM
//! ```
//!
//! Two consequences are definitional, NOT bugs:
//!
//! - A **wash / round-trip** (`A→B` then `B→A`) is a pure cycle → nets to `0`.
//!   That is also precisely how the wash-trading literature identifies a wash:
//!   a cycle (strongly connected component) whose participants end at zero
//!   balance.
//! - Two *intent-wise unrelated* payments that happen to offset (`A→B`, `C→A`)
//!   decompose into the single path `C→A→B` → the value is that path's, not the
//!   sum of the two legs. The arithmetic cannot see intent, and does not try to.
//!
//! Per-account delta netting is also exactly the multilateral-netting algorithm
//! clearing houses have used for decades (owe 50, owed 48 → settle the residual
//! 2, not 98). If a gross figure is ever wanted alongside, the theorem gives it
//! for free: `cycle volume (wash) = gross − net`.
//!
//! This reducer is pure: the asset is an already-resolved `i64` surrogate and
//! the accounts are their StrKey identities. Two rules from the formula are the
//! caller's responsibility, because they are resolution concerns, not arithmetic:
//!
//! - **Native XLM canonicalised to one `asset_id`** — the caller passes the
//!   single native surrogate (`hash64("native")`) for both native conventions
//!   so the deltas cancel.
//! - **`fee` events excluded** — the caller drops fee movements before calling.

use std::collections::BTreeMap;

/// One signed value movement within a single transaction.
///
/// `from`/`to` are account identities: a `G…` account StrKey, or a `C…`
/// contract address (a C→C transfer must net correctly, so contracts are not
/// dropped here). `None` marks a one-sided event: `from == None` is a **mint**
/// (value created), `to == None` is a **burn/clawback** (value destroyed).
/// `amount` is the raw, unscaled token quantity (non-negative; decimals are
/// applied at read time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movement {
    pub asset_id: i64,
    pub from: Option<String>,
    pub to: Option<String>,
    pub amount: i128,
}

/// Net-settled value for one (transaction, asset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSettled {
    pub asset_id: i64,
    /// `max(Σ positive deltas, Σ negative deltas)`, raw (unscaled).
    ///
    /// `None` when the reduction did not fit in `i128`. Amounts come from ledger
    /// balance deltas; for a **bespoke** token the stored balance is
    /// contract-authored, so it is still attacker-controlled — two movements of
    /// `i128::MAX` in one transaction are one transaction's worth of effort.
    /// Release builds have `overflow-checks = false`, so unchecked arithmetic
    /// would not panic — it would wrap and store a wrong figure as money. `None`
    /// means "not representable, not computed", which is what the nullable storage
    /// column exists to carry.
    pub amount: Option<i128>,
}

/// Reduce a transaction's value movements to the net-settled value per asset.
///
/// Returns one [`NetSettled`] per distinct `asset_id`, ordered by `asset_id`
/// for deterministic output. An empty input yields an empty vec.
pub fn net_settled(movements: &[Movement]) -> Vec<NetSettled> {
    // asset_id -> (account -> signed delta). A `None` delta marks an account
    // whose running balance left i128; it poisons its asset's whole result,
    // because the reduction below can no longer be computed honestly.
    let mut per_asset: BTreeMap<i64, BTreeMap<&str, Option<i128>>> = BTreeMap::new();
    for m in movements {
        let deltas = per_asset.entry(m.asset_id).or_default();
        if let Some(to) = &m.to {
            let d = deltas.entry(to.as_str()).or_insert(Some(0));
            *d = d.and_then(|v| v.checked_add(m.amount));
        }
        if let Some(from) = &m.from {
            let d = deltas.entry(from.as_str()).or_insert(Some(0));
            *d = d.and_then(|v| v.checked_sub(m.amount));
        }
    }

    per_asset
        .into_iter()
        .map(|(asset_id, deltas)| NetSettled {
            asset_id,
            amount: reduce(&deltas),
        })
        .collect()
}

/// `max(Σ positive deltas, Σ negative deltas)`, or `None` if any account delta
/// overflowed or either side's sum does. Every step is checked: an unrepresentable
/// total must surface as "not computed", never as a wrapped value.
fn reduce(deltas: &BTreeMap<&str, Option<i128>>) -> Option<i128> {
    let mut gained: i128 = 0;
    let mut lost: i128 = 0;
    for delta in deltas.values() {
        let d = (*delta)?;
        if d > 0 {
            gained = gained.checked_add(d)?;
        } else if d < 0 {
            // `checked_neg` because -i128::MIN is itself unrepresentable.
            lost = lost.checked_add(d.checked_neg()?)?;
        }
    }
    Some(gained.max(lost))
}

#[cfg(test)]
mod tests {
    use super::*;

    const X: i64 = 100; // asset X surrogate
    const Y: i64 = 200; // asset Y surrogate
    const A: &str = "A";
    const B: &str = "B";
    const C: &str = "C";
    const D: &str = "D";
    const E: &str = "E";
    const ISSUER: &str = "ISSUER";

    fn transfer(asset: i64, from: &str, to: &str, amount: i128) -> Movement {
        Movement {
            asset_id: asset,
            from: Some(from.to_owned()),
            to: Some(to.to_owned()),
            amount,
        }
    }
    fn mint(asset: i64, to: &str, amount: i128) -> Movement {
        Movement {
            asset_id: asset,
            from: None,
            to: Some(to.to_owned()),
            amount,
        }
    }
    fn burn(asset: i64, from: &str, amount: i128) -> Movement {
        Movement {
            asset_id: asset,
            from: Some(from.to_owned()),
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
        assert_eq!(net_settled(&[]), vec![]);
    }

    #[test]
    fn plain_transfer_is_its_amount() {
        let r = net_settled(&[transfer(X, A, B, 100)]);
        let n = one(&r);
        assert_eq!((n.asset_id, n.amount), (X, Some(100)));
    }

    #[test]
    fn routing_chain_nets_to_moved_value_not_gross() {
        // A -> B -> C, 100. Gross would be 200; net is 100 (B passes through).
        let r = net_settled(&[transfer(X, A, B, 100), transfer(X, B, C, 100)]);
        assert_eq!(one(&r).amount, Some(100));
    }

    #[test]
    fn pure_mint_counts_the_created_value() {
        let n = &net_settled(&[mint(X, A, 100)])[0];
        assert_eq!(n.amount, Some(100));
    }

    #[test]
    fn pure_burn_counts_the_destroyed_value() {
        // Σ+ alone would give 0 here; max(Σ+, Σ−) keeps it at 100.
        let n = &net_settled(&[burn(X, A, 100)])[0];
        assert_eq!(n.amount, Some(100));
    }

    #[test]
    fn transfer_plus_burn_takes_the_larger_side() {
        // transfer 100 (A->B) + burn 40 (B). Σ+ = 60, Σ− = 100 -> 100.
        let r = net_settled(&[transfer(X, A, B, 100), burn(X, B, 40)]);
        assert_eq!(one(&r).amount, Some(100));
    }

    #[test]
    fn redeem_to_issuer_as_two_sided_transfer() {
        // redeem 250 recorded as a transfer to the issuer.
        let n = &net_settled(&[transfer(X, A, ISSUER, 250)])[0];
        assert_eq!(n.amount, Some(250));
    }

    #[test]
    fn redeem_one_sided_is_representation_robust() {
        // Same redeem recorded one-sided (burn-shaped) still nets to 250.
        let n = &net_settled(&[burn(X, A, 250)])[0];
        assert_eq!(n.amount, Some(250));
    }

    #[test]
    fn self_cancelling_mint_transfer_burn_nets_to_zero() {
        // mint 100 -> A, A -> B 100, burn 100 from B. Every account nets to 0.
        let r = net_settled(&[mint(X, A, 100), transfer(X, A, B, 100), burn(X, B, 100)]);
        assert_eq!(one(&r).amount, Some(0));
    }

    #[test]
    fn two_assets_split_into_two_rows_ordered_by_asset_id() {
        // asset X transfer 100, asset Y transfer 50, in one transaction.
        let r = net_settled(&[transfer(Y, B, A, 50), transfer(X, A, B, 100)]);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].asset_id, r[0].amount), (X, Some(100))); // ordered by asset_id
        assert_eq!((r[1].asset_id, r[1].amount), (Y, Some(50)));
    }

    #[test]
    fn independent_transfers_of_same_asset_sum() {
        // A->B 100 and C->D 30 share no account: net is 130, not 100.
        let r = net_settled(&[transfer(X, A, B, 100), transfer(X, C, D, 30)]);
        assert_eq!(one(&r).amount, Some(130));
    }

    #[test]
    fn fan_out_through_a_partial_pass_through_hub() {
        //   A --100--> B          B is a PARTIAL pass-through: it receives 100
        //   B ---60--> C          but sends 180, so it funds 80 of its own.
        //   B ---20--> D
        //   B --100--> E          deltas: A -100, B -80, C +50, D +20, E +110
        //   C ---10--> E          gained = 50+20+110 = 180
        //                         lost   = 100+80    = 180  -> 180
        // Gross (sum of legs) would be 290. A DAG, no cycles, so the flow value
        // is the full 180 — the hub is not erased, only its pass-through part.
        let r = net_settled(&[
            transfer(X, A, B, 100),
            transfer(X, B, C, 60),
            transfer(X, B, D, 20),
            transfer(X, B, E, 100),
            transfer(X, C, E, 10),
        ]);
        assert_eq!(one(&r).amount, Some(180));
    }

    // ---- overflow: amounts are contract-emitted, i.e. attacker-chosen --------

    #[test]
    fn accumulator_overflow_is_reported_as_not_computed_not_wrapped() {
        // Two mints of i128::MAX to the same account. Unchecked, the running
        // delta wraps (release has overflow-checks = false) and we would store
        // a negative-looking figure as money. Cost of this input: one tx.
        let r = net_settled(&[mint(X, A, i128::MAX), mint(X, A, i128::MAX)]);
        assert_eq!(
            one(&r).amount,
            None,
            "must not wrap into a fabricated value"
        );
    }

    #[test]
    fn sum_overflow_across_accounts_is_reported_as_not_computed() {
        // Each account's delta fits; the Σ+ across them does not.
        let r = net_settled(&[mint(X, A, i128::MAX), mint(X, B, i128::MAX)]);
        assert_eq!(one(&r).amount, None);
    }

    #[test]
    fn negation_of_i128_min_does_not_overflow() {
        // Σ− negates each negative delta; -i128::MIN is itself unrepresentable,
        // so a burn of i128::MIN must yield None rather than panic/wrap.
        let r = net_settled(&[Movement {
            asset_id: X,
            from: Some(A.to_owned()),
            to: None,
            amount: i128::MIN,
        }]);
        assert_eq!(one(&r).amount, None);
    }

    #[test]
    fn one_overflowing_asset_does_not_poison_the_others() {
        // Asset X overflows; asset Y in the same transaction stays computable.
        let r = net_settled(&[
            mint(X, A, i128::MAX),
            mint(X, A, i128::MAX),
            transfer(Y, A, B, 50),
        ]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].amount, None, "X overflowed");
        assert_eq!(r[1].amount, Some(50), "Y is unaffected");
    }

    #[test]
    fn pure_cycle_is_zero_by_flow_decomposition() {
        // A->B 100 then B->A 100: a 2-cycle. The flow decomposition theorem
        // gives a cycle zero contribution to the flow value, so net is 0 even
        // though 200 moved gross. This is also the wash-trading signature
        // (cycle whose members end at zero balance) — definitional, not a bug.
        let r = net_settled(&[transfer(X, A, B, 100), transfer(X, B, A, 100)]);
        assert_eq!(one(&r).amount, Some(0));
    }
}
