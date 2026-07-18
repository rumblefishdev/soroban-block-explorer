//! Classic (non-Soroban) value movement per (transaction, asset), derived from
//! ledger-entry balance changes (task 0393).
//!
//! `AccountEntry` (native XLM) and `TrustLineEntry` (issued assets) are the only
//! balance carriers in classic Stellar. **Every** classic value flow — payment,
//! path payment, offer/DEX fill, liquidity-pool deposit/withdraw, claimable-
//! balance create/claim, clawback — settles as a change to one or both, so a
//! single before→after delta reader over `TransactionMeta` covers all classic
//! operation types uniformly and auto-nets routing hops (a pass-through account
//! ends at delta 0). This is the "cleanest source" of the task design and why
//! `meta.rs` (the version-safe change accessor) was revived.
//!
//! ## Fee
//!
//! The transaction fee is charged in the ledger's separate `feeProcessing`
//! phase, **not** in `TransactionMeta` (the apply phase). So these deltas never
//! include the fee — formula rule 3 ("fee events excluded") is satisfied by the
//! source, with no subtraction. (A seq-number bump on the source appears in
//! `tx_changes_before`, but it does not move `balance`, so it nets to a 0 delta
//! and is dropped.)
//!
//! Output is per-(account, asset) signed deltas; the caller resolves the asset
//! surrogate and reduces to `max(Σ+, Σ−)` per asset via `xdr_parser::net_settled`.

use std::collections::BTreeMap;

use stellar_xdr::{
    LedgerEntry, LedgerEntryChange, LedgerEntryData, LedgerKey, TransactionMeta, TrustLineAsset,
};

use crate::event_filters::EventAsset;
use crate::meta::ledger_changes;

/// A net balance change for one account on one asset within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassicDelta {
    /// Account G-StrKey.
    pub account: String,
    /// The moved asset — `Native` or `Credit { code, issuer }` (classic never
    /// yields `Contract`). This is the same `EventAsset` the Soroban token path
    /// decodes, so the caller resolves both to a surrogate through one helper.
    pub asset: EventAsset,
    /// Signed raw stroops the account's balance moved across the transaction.
    pub delta: i128,
}

/// Per-(account, asset) net classic balance delta for a transaction. Only
/// non-zero deltas are returned, ordered by (account, asset) for determinism.
pub fn classic_balance_deltas(meta: &TransactionMeta) -> Vec<ClassicDelta> {
    // (account, asset) -> before/after balance, telescoped across the tx.
    let mut acc: BTreeMap<(String, EventAsset), Balances> = BTreeMap::new();
    for change in ledger_changes(meta) {
        match change {
            // `State` / `Restored` are before-images (Restored re-appears from
            // state archival, protocol 23 — the restore itself moves no value).
            LedgerEntryChange::State(e) | LedgerEntryChange::Restored(e) => {
                record(&mut acc, e, false);
            }
            LedgerEntryChange::Created(e) => record(&mut acc, e, true),
            LedgerEntryChange::Updated(e) => record(&mut acc, e, false),
            LedgerEntryChange::Removed(k) => record_removed(&mut acc, k),
        }
    }
    acc.into_iter()
        .filter_map(|((account, asset), b)| {
            let delta = b.last - b.initial.unwrap_or(0);
            (delta != 0).then_some(ClassicDelta {
                account,
                asset,
                delta,
            })
        })
        .collect()
}

/// Running (initial, last) balance for one (account, asset) key.
struct Balances {
    initial: Option<i128>,
    last: i128,
}

/// Fold a balance-bearing entry into the running map. `created` marks a
/// `Created` change (the entry did not exist before → initial balance 0);
/// otherwise the first change seen (a `State` before-image, or an `Updated`
/// with no preceding state) sets the initial balance.
fn record(acc: &mut BTreeMap<(String, EventAsset), Balances>, entry: &LedgerEntry, created: bool) {
    let Some((account, asset, balance)) = entry_balance(entry) else {
        return;
    };
    let e = acc.entry((account, asset)).or_insert(Balances {
        initial: None,
        last: 0,
    });
    if e.initial.is_none() {
        e.initial = Some(if created { 0 } else { balance });
    }
    e.last = balance;
}

/// A removed entry drops to balance 0 (its initial came from a preceding State).
fn record_removed(acc: &mut BTreeMap<(String, EventAsset), Balances>, key: &LedgerKey) {
    let Some((account, asset)) = removed_balance_key(key) else {
        return;
    };
    acc.entry((account, asset))
        .or_insert(Balances {
            initial: None,
            last: 0,
        })
        .last = 0;
}

/// `(account, asset, balance)` for the two balance-bearing entry types; `None`
/// for every other entry (offers, LP, claimable balances, contract data, …) —
/// their value effects surface as account/trustline balance changes anyway.
fn entry_balance(entry: &LedgerEntry) -> Option<(String, EventAsset, i128)> {
    match &entry.data {
        LedgerEntryData::Account(a) => Some((
            a.account_id.to_string(),
            EventAsset::Native,
            i128::from(a.balance),
        )),
        LedgerEntryData::Trustline(t) => Some((
            t.account_id.to_string(),
            trustline_event_asset(&t.asset)?,
            i128::from(t.balance),
        )),
        _ => None,
    }
}

/// `(account, asset)` for a removed account/trustline key; `None` otherwise.
fn removed_balance_key(key: &LedgerKey) -> Option<(String, EventAsset)> {
    match key {
        LedgerKey::Account(k) => Some((k.account_id.to_string(), EventAsset::Native)),
        LedgerKey::Trustline(k) => {
            Some((k.account_id.to_string(), trustline_event_asset(&k.asset)?))
        }
        _ => None,
    }
}

/// The `EventAsset` identity of a trustline asset; `None` for pool shares (not a
/// single-asset balance). `Native` is included for completeness though native
/// balances live on `AccountEntry`, not a trustline.
fn trustline_event_asset(asset: &TrustLineAsset) -> Option<EventAsset> {
    match asset {
        TrustLineAsset::Native => Some(EventAsset::Native),
        TrustLineAsset::CreditAlphanum4(a) => Some(EventAsset::Credit {
            code: crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            issuer: a.issuer.to_string(),
        }),
        TrustLineAsset::CreditAlphanum12(a) => Some(EventAsset::Credit {
            code: crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            issuer: a.issuer.to_string(),
        }),
        TrustLineAsset::PoolShare(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        AccountEntry, AccountEntryExt, AccountId, AlphaNum4, AssetCode4, ExtensionPoint, Hash,
        LedgerEntryChanges, LedgerEntryExt, LedgerKeyTrustLine, OperationMeta, PoolId, PublicKey,
        SequenceNumber, String32, Thresholds, TransactionMetaV3, TrustLineEntry, TrustLineEntryExt,
        Uint256, VecM,
    };

    fn acct_id(b: u8) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([b; 32])))
    }
    fn strkey(b: u8) -> String {
        acct_id(b).to_string()
    }

    fn account_entry(id: u8, balance: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 100,
            data: LedgerEntryData::Account(AccountEntry {
                account_id: acct_id(id),
                balance,
                seq_num: SequenceNumber(1),
                num_sub_entries: 0,
                inflation_dest: None,
                flags: 0,
                home_domain: String32::default(),
                thresholds: Thresholds([1, 0, 0, 0]),
                signers: VecM::default(),
                ext: AccountEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    fn usdc_asset(issuer: u8) -> TrustLineAsset {
        TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*b"USDC"),
            issuer: acct_id(issuer),
        })
    }

    fn trustline_entry(holder: u8, asset: TrustLineAsset, balance: i64) -> LedgerEntry {
        LedgerEntry {
            last_modified_ledger_seq: 100,
            data: LedgerEntryData::Trustline(TrustLineEntry {
                account_id: acct_id(holder),
                asset,
                balance,
                limit: i64::MAX,
                flags: 1,
                ext: TrustLineEntryExt::V0,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    /// Build a V3 meta whose single operation carries `changes`.
    fn meta_with_op_changes(changes: Vec<LedgerEntryChange>) -> TransactionMeta {
        TransactionMeta::V3(TransactionMetaV3 {
            ext: ExtensionPoint::V0,
            tx_changes_before: LedgerEntryChanges::default(),
            operations: vec![OperationMeta {
                changes: changes.try_into().unwrap(),
            }]
            .try_into()
            .unwrap(),
            tx_changes_after: LedgerEntryChanges::default(),
            soroban_meta: None,
        })
    }

    fn find<'a>(
        d: &'a [ClassicDelta],
        account: &str,
        asset: &EventAsset,
    ) -> Option<&'a ClassicDelta> {
        d.iter().find(|x| x.account == account && &x.asset == asset)
    }

    fn usdc_credit() -> EventAsset {
        EventAsset::Credit {
            code: "USDC".to_string(),
            issuer: strkey(0x11),
        }
    }

    #[test]
    fn native_payment_nets_source_and_destination() {
        // source 1000 -> 900, dest 500 -> 600 (payment of 100 native).
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 900)),
            LedgerEntryChange::State(account_entry(0xBB, 500)),
            LedgerEntryChange::Updated(account_entry(0xBB, 600)),
        ]);
        let d = classic_balance_deltas(&meta);
        assert_eq!(
            find(&d, &strkey(0xAA), &EventAsset::Native).unwrap().delta,
            -100
        );
        assert_eq!(
            find(&d, &strkey(0xBB), &EventAsset::Native).unwrap().delta,
            100
        );
    }

    #[test]
    fn created_account_funding_moves_native() {
        // funder 1000 -> 700, new account created at 300.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 700)),
            LedgerEntryChange::Created(account_entry(0xCC, 300)),
        ]);
        let d = classic_balance_deltas(&meta);
        assert_eq!(
            find(&d, &strkey(0xAA), &EventAsset::Native).unwrap().delta,
            -300
        );
        assert_eq!(
            find(&d, &strkey(0xCC), &EventAsset::Native).unwrap().delta,
            300
        );
    }

    #[test]
    fn credit_trustline_payment_uses_credit_asset() {
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(trustline_entry(0xAA, usdc_asset(0x11), 1000)),
            LedgerEntryChange::Updated(trustline_entry(0xAA, usdc_asset(0x11), 850)),
        ]);
        let d = classic_balance_deltas(&meta);
        assert_eq!(find(&d, &strkey(0xAA), &usdc_credit()).unwrap().delta, -150);
    }

    #[test]
    fn removed_trustline_zeroes_the_balance() {
        // trustline 100 -> removed: delta -100.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(trustline_entry(0xAA, usdc_asset(0x11), 100)),
            LedgerEntryChange::Removed(LedgerKey::Trustline(LedgerKeyTrustLine {
                account_id: acct_id(0xAA),
                asset: usdc_asset(0x11),
            })),
        ]);
        let d = classic_balance_deltas(&meta);
        assert_eq!(find(&d, &strkey(0xAA), &usdc_credit()).unwrap().delta, -100);
    }

    #[test]
    fn balance_unchanged_update_is_dropped() {
        // seq-number bump only: balance 1000 -> 1000, delta 0, no row.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 1000)),
        ]);
        assert!(classic_balance_deltas(&meta).is_empty());
    }

    #[test]
    fn only_the_first_image_sets_initial_across_repeated_changes() {
        // Two ops touch the same account, each emitting its own State/Updated
        // pair: 1000 -> 900, then 900 -> 850. Expected: ONE row, delta -150,
        // measured first image to last.
        //
        // This pins the `if initial.is_none()` guard specifically. Drop it and
        // the second State(900) overwrites initial, so the answer collapses to
        // 850 - 850 = 0 and the whole transaction reads as moving nothing. (It
        // does NOT distinguish telescoping from summing per-step deltas —
        // (900-1000)+(850-900) is -150 either way; that is the telescoping
        // identity, not a difference worth testing.)
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 900)),
            LedgerEntryChange::State(account_entry(0xAA, 900)),
            LedgerEntryChange::Updated(account_entry(0xAA, 850)),
        ]);
        let d = classic_balance_deltas(&meta);
        assert_eq!(d.len(), 1, "one row per (account, asset), got {d:?}");
        assert_eq!(
            find(&d, &strkey(0xAA), &EventAsset::Native).unwrap().delta,
            -150
        );
    }

    #[test]
    fn restored_is_a_before_image_not_a_value_move() {
        // Protocol 23 state archival: the entry re-appears via Restored, which
        // moves no value by itself — it is the "before" for what follows.
        // Restored(1000) alone -> no row; Restored(1000) + Updated(900) -> -100.
        let alone =
            meta_with_op_changes(vec![LedgerEntryChange::Restored(account_entry(0xAA, 1000))]);
        assert!(
            classic_balance_deltas(&alone).is_empty(),
            "a bare restore moves nothing"
        );

        let then_spent = meta_with_op_changes(vec![
            LedgerEntryChange::Restored(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 900)),
        ]);
        let d = classic_balance_deltas(&then_spent);
        assert_eq!(
            find(&d, &strkey(0xAA), &EventAsset::Native).unwrap().delta,
            -100
        );
    }

    #[test]
    fn pool_share_trustline_is_not_a_single_asset_balance() {
        // A pool-share trustline balance is LP shares, not an asset amount —
        // counting it would invent value on every LP deposit/withdraw.
        let pool_share = TrustLineAsset::PoolShare(PoolId(Hash([0x22; 32])));
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(trustline_entry(0xAA, pool_share.clone(), 100)),
            LedgerEntryChange::Updated(trustline_entry(0xAA, pool_share, 250)),
        ]);
        assert!(classic_balance_deltas(&meta).is_empty());
    }
}
