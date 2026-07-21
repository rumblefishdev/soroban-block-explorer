//! Value movement per (transaction, asset), derived from LEDGER balance-entry
//! changes — the authoritative, consensus-verified source (a contract cannot forge
//! a ledger balance). Token EVENTS are logs and are NEVER used for value (task
//! 0393 redesign; see 0415). Covers EVERY tx, classic or Soroban.
//!
//! The balance carriers:
//! - `AccountEntry.balance` (native XLM),
//! - `TrustLineEntry.balance` (classic issued assets),
//! - `ContractData` `Balance(Address)` — a Soroban token balance held by an
//!   account or contract: a SAC `BalanceValue` **struct** (a classic/native asset
//!   held by a contract) or a **bare `i128`** (a bespoke token, which IS the asset).
//!
//! **Every** value flow — payment, path payment, offer/DEX fill, LP deposit/
//! withdraw, claimable-balance create/claim, clawback, and Soroban SAC/bespoke
//! transfers — settles as a change to one or more of these, so a single
//! before→after delta reader over `TransactionMeta` covers all of them uniformly
//! and auto-nets routing hops (a pass-through holder ends at delta 0).
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
    ContractDataEntry, LedgerEntry, LedgerEntryChange, LedgerEntryData, LedgerKey, ScVal,
    TransactionMeta, TrustLineAsset,
};

use crate::meta::ledger_changes;

/// The asset a LEDGER balance change moved — the LEDGER domain's asset vocabulary
/// (cf. `AssetRef` for op-declared assets, `EventAsset` for event-named; each domain
/// owns its own small asset enum). Resolved to a DB surrogate by the persistence
/// layer (`ids` / `sac_classic`). `Ord` so it can key the telescoping `BTreeMap`.
///
/// The two `ContractData` cases are **different DB asset types** — a SAC-wrapped
/// classic (type 1) vs a bespoke token (type 3) — so they are two variants, not one
/// variant with a boolean: each resolves a different way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LedgerAsset {
    /// Native XLM — from `AccountEntry`.
    Native,
    /// A classic issued asset (code + issuer `G…` StrKey) — from `TrustLineEntry`.
    Credit { code: String, issuer: String },
    /// A classic/native asset held via its Stellar-Asset-Contract **wrapper** — from
    /// a `ContractData` `Balance` whose value is a SAC `BalanceValue` struct. The
    /// `C…` is the SAC contract; the caller reverses it to the wrapped classic
    /// `asset_id` via the registry (`sac_classic`), or drops if unknown. A **classic**
    /// asset (DB type 1), NOT a contract asset.
    SacWrapped(String),
    /// A **bespoke** Soroban token — from a `ContractData` `Balance` whose value is a
    /// bare `i128`. The `C…` token contract IS the asset (DB type 3); resolved to its
    /// own surrogate (`ids::contract_id`).
    Bespoke(String),
}

/// A net balance change for one account on one asset within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerDelta {
    /// Account G-StrKey.
    pub account: String,
    /// The moved asset. `ledger_balance_deltas` yields `Native` / `Credit` (from
    /// `AccountEntry` / `TrustLineEntry` changes) and `SacWrapped` / `Bespoke` (from
    /// `ContractData` balance changes); the caller resolves each to a surrogate.
    /// Never `Contract` — that identity comes only from an event, and the ledger
    /// reader does not read events.
    pub asset: LedgerAsset,
    /// Signed raw stroops the account's balance moved across the transaction.
    pub delta: i128,
}

/// Per-(account, asset) net classic balance delta for a transaction. Only
/// non-zero deltas are returned, ordered by (account, asset) for determinism.
pub fn ledger_balance_deltas(meta: &TransactionMeta) -> Vec<LedgerDelta> {
    // Runs for EVERY tx (classic AND Soroban): the ledger is the authoritative,
    // unspoofable source of value for native/classic/SAC, whether moved by a
    // classic op (Account/Trustline changes) or a Soroban invocation (which also
    // produces contract-held SAC balance changes in `ContractData`). Only bespoke
    // tokens — with no ledger-readable balance — are valued from their events.
    // (account, asset) -> before/after balance, telescoped across the tx.
    let mut acc: BTreeMap<(String, LedgerAsset), Balances> = BTreeMap::new();
    for change in ledger_changes(meta) {
        match change {
            // `State` / `Restored` are before-images (Restored re-appears from
            // state archival, protocol 23 — the restore itself moves no value).
            LedgerEntryChange::State(e) | LedgerEntryChange::Restored(e) => {
                record(&mut acc, e, false, false);
            }
            LedgerEntryChange::Created(e) => record(&mut acc, e, true, false),
            LedgerEntryChange::Updated(e) => record(&mut acc, e, false, true),
            LedgerEntryChange::Removed(k) => record_removed(&mut acc, k),
        }
    }
    acc.into_iter()
        .filter_map(|((account, asset), b)| {
            // A bespoke token's `ContractData` balance is attacker-authored i128,
            // so this subtraction can overflow; release builds run with
            // `overflow-checks = false`, so an unchecked `-` would wrap into a
            // fabricated figure instead of panicking. `checked_sub` → `?` drops
            // the delta (no value) on overflow rather than lying. (Native/classic
            // balances are i64-wide and cannot overflow here.)
            let delta = b.last.checked_sub(b.initial.unwrap_or(0))?;
            (delta != 0).then_some(LedgerDelta {
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
fn record(
    acc: &mut BTreeMap<(String, LedgerAsset), Balances>,
    entry: &LedgerEntry,
    created: bool,
    is_update: bool,
) {
    let Some((account, asset, balance)) = entry_balance(entry) else {
        return;
    };
    let e = acc.entry((account, asset)).or_insert(Balances {
        initial: None,
        last: 0,
    });
    if e.initial.is_none() {
        // Telescoping invariant (#11): the FIRST change for a key must be a
        // before-image (`State`/`Restored`) or a `Created` — never an `Updated`.
        // If an `Updated` were first, `balance` is the AFTER value and using it as
        // `initial` silently zeroes that step's delta. Stellar always emits a
        // `State` before an `Updated`, so this is debug-only (zero release cost);
        // it fails a test/dev build fast if a malformed or future-protocol meta
        // ever violates it, instead of understating in silence.
        debug_assert!(
            !is_update,
            "classic telescoping: Updated with no preceding State/Created — pre-image lost, delta understated"
        );
        e.initial = Some(if created { 0 } else { balance });
    }
    e.last = balance;
}

/// A removed entry drops to balance 0 (its initial came from a preceding State).
fn record_removed(acc: &mut BTreeMap<(String, LedgerAsset), Balances>, key: &LedgerKey) {
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

/// `(account, asset, balance)` for the balance-bearing entry types:
/// `AccountEntry` (native), `TrustLineEntry` (classic credit), and a SAC
/// contract-held balance (`ContractData` `Balance(Address)` with a SAC
/// `BalanceValue` struct value). `None` for everything else (offers, LP,
/// claimable balances, bespoke ContractData) — their effects either surface as
/// account/trustline changes or belong to the bespoke event path.
fn entry_balance(entry: &LedgerEntry) -> Option<(String, LedgerAsset, i128)> {
    match &entry.data {
        LedgerEntryData::Account(a) => Some((
            a.account_id.to_string(),
            LedgerAsset::Native,
            i128::from(a.balance),
        )),
        LedgerEntryData::Trustline(t) => Some((
            t.account_id.to_string(),
            trustline_event_asset(&t.asset)?,
            i128::from(t.balance),
        )),
        LedgerEntryData::ContractData(cd) => contract_data_balance(cd),
        _ => None,
    }
}

/// `(account, asset)` for a removed balance key; `None` otherwise. Covers a
/// removed account/trustline and a removed SAC contract-balance entry
/// (`ContractData` `Balance(Address)` dropping to 0).
fn removed_balance_key(key: &LedgerKey) -> Option<(String, LedgerAsset)> {
    match key {
        LedgerKey::Account(k) => Some((k.account_id.to_string(), LedgerAsset::Native)),
        LedgerKey::Trustline(k) => {
            Some((k.account_id.to_string(), trustline_event_asset(&k.asset)?))
        }
        // Removed carries no value, so it cannot tell a SAC balance (struct) from a
        // bespoke one (bare i128) — the distinction lives in the value it dropped.
        // A balance normally hits 0 via `Updated`, not `Removed`, so this rare edge
        // is left unhandled rather than risk keying the wrong asset variant.
        _ => None,
    }
}

/// A Soroban token balance change: a `ContractData` entry with key
/// `Balance(Address)`. Two value shapes, both authoritative (the actual stored
/// balance, unspoofable):
/// - a SAC `BalanceValue` **struct** (`Map{ amount, authorized, clawback }`) — a
///   contract holding a classic/native asset → `SacWrapped(sac StrKey)` (the caller
///   re-maps it to the wrapped classic asset).
/// - a **bare `i128`** — a bespoke Soroban token's own balance →
///   `Bespoke(token StrKey)` (the token IS the asset).
///
/// `None` for any other `ContractData` (TTL bumps, non-`Balance` keys, unknown
/// value shapes). Verified against a real mainnet Soroban SAC transfer
/// (contract↔contract): `key` = `Vec[Symbol("Balance"), Address(holder)]`, `val` =
/// the struct, emitted `State`(before) + `Updated`(after) so it telescopes.
fn contract_data_balance(cd: &ContractDataEntry) -> Option<(String, LedgerAsset, i128)> {
    let holder = balance_key_holder(&cd.key)?;
    let contract = cd.contract.to_string();
    if let Some(amount) = sac_balance_struct_amount(&cd.val) {
        // Struct value → a classic/native asset held via its SAC wrapper.
        return Some((holder, LedgerAsset::SacWrapped(contract), amount));
    }
    if let ScVal::I128(p) = &cd.val {
        // Bare i128 → a bespoke token; the contract IS the asset.
        let amount = (i128::from(p.hi) << 64) | i128::from(p.lo);
        return Some((holder, LedgerAsset::Bespoke(contract), amount));
    }
    None
}

/// The holder StrKey from a `Balance(Address)` `ContractData` key
/// (`Vec[Symbol("Balance"), Address(holder)]`); `None` for any other key shape.
fn balance_key_holder(key: &ScVal) -> Option<String> {
    let ScVal::Vec(Some(v)) = key else {
        return None;
    };
    let [tag, holder] = v.as_slice() else {
        return None;
    };
    match tag {
        ScVal::Symbol(s) if s.to_string() == "Balance" => {}
        _ => return None,
    }
    match holder {
        ScVal::Address(addr) => Some(addr.to_string()),
        _ => None,
    }
}

/// The `amount` field of a SAC `BalanceValue` struct (`ScVal::Map`); `None` for a
/// bare i128 (bespoke) or any other value shape.
fn sac_balance_struct_amount(val: &ScVal) -> Option<i128> {
    let ScVal::Map(Some(m)) = val else {
        return None;
    };
    for entry in m.iter() {
        if let ScVal::Symbol(k) = &entry.key
            && k.to_string() == "amount"
            && let ScVal::I128(p) = &entry.val
        {
            return Some((i128::from(p.hi) << 64) | i128::from(p.lo));
        }
    }
    None
}

/// The `LedgerAsset` identity of a trustline asset; `None` for pool shares (not a
/// single-asset balance). `Native` is included for completeness though native
/// balances live on `AccountEntry`, not a trustline.
fn trustline_event_asset(asset: &TrustLineAsset) -> Option<LedgerAsset> {
    match asset {
        TrustLineAsset::Native => Some(LedgerAsset::Native),
        TrustLineAsset::CreditAlphanum4(a) => Some(LedgerAsset::Credit {
            code: crate::asset_code::asset_code_str(a.asset_code.as_slice()),
            issuer: a.issuer.to_string(),
        }),
        TrustLineAsset::CreditAlphanum12(a) => Some(LedgerAsset::Credit {
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
        d: &'a [LedgerDelta],
        account: &str,
        asset: &LedgerAsset,
    ) -> Option<&'a LedgerDelta> {
        d.iter().find(|x| x.account == account && &x.asset == asset)
    }

    fn usdc_credit() -> LedgerAsset {
        LedgerAsset::Credit {
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
        let d = ledger_balance_deltas(&meta);
        assert_eq!(
            find(&d, &strkey(0xAA), &LedgerAsset::Native).unwrap().delta,
            -100
        );
        assert_eq!(
            find(&d, &strkey(0xBB), &LedgerAsset::Native).unwrap().delta,
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
        let d = ledger_balance_deltas(&meta);
        assert_eq!(
            find(&d, &strkey(0xAA), &LedgerAsset::Native).unwrap().delta,
            -300
        );
        assert_eq!(
            find(&d, &strkey(0xCC), &LedgerAsset::Native).unwrap().delta,
            300
        );
    }

    #[test]
    fn credit_trustline_payment_uses_credit_asset() {
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(trustline_entry(0xAA, usdc_asset(0x11), 1000)),
            LedgerEntryChange::Updated(trustline_entry(0xAA, usdc_asset(0x11), 850)),
        ]);
        let d = ledger_balance_deltas(&meta);
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
        let d = ledger_balance_deltas(&meta);
        assert_eq!(find(&d, &strkey(0xAA), &usdc_credit()).unwrap().delta, -100);
    }

    #[test]
    fn balance_unchanged_update_is_dropped() {
        // seq-number bump only: balance 1000 -> 1000, delta 0, no row.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 1000)),
        ]);
        assert!(ledger_balance_deltas(&meta).is_empty());
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
        let d = ledger_balance_deltas(&meta);
        assert_eq!(d.len(), 1, "one row per (account, asset), got {d:?}");
        assert_eq!(
            find(&d, &strkey(0xAA), &LedgerAsset::Native).unwrap().delta,
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
            ledger_balance_deltas(&alone).is_empty(),
            "a bare restore moves nothing"
        );

        let then_spent = meta_with_op_changes(vec![
            LedgerEntryChange::Restored(account_entry(0xAA, 1000)),
            LedgerEntryChange::Updated(account_entry(0xAA, 900)),
        ]);
        let d = ledger_balance_deltas(&then_spent);
        assert_eq!(
            find(&d, &strkey(0xAA), &LedgerAsset::Native).unwrap().delta,
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
        assert!(ledger_balance_deltas(&meta).is_empty());
    }

    // ---- ContractData Soroban balances (task 0393 ledger redesign) --------
    // Synthetic coverage of `contract_data_balance` / `sac_balance_struct_amount`
    // / `balance_key_holder`. (A real-mainnet decode also lives in
    // `tests/net_settled_ledger_contractdata.rs`, gated on a captured fixture.)

    fn contract_addr(b: u8) -> stellar_xdr::ScAddress {
        stellar_xdr::ScAddress::Contract(stellar_xdr::ContractId(Hash([b; 32])))
    }

    fn i128_scval(v: i128) -> ScVal {
        ScVal::I128(stellar_xdr::Int128Parts {
            hi: (v >> 64) as i64,
            lo: v as u64,
        })
    }

    /// A SAC `BalanceValue` struct value: `Map{ amount, authorized, clawback }`.
    fn sac_balance_struct(amount: i128) -> ScVal {
        use stellar_xdr::{ScMap, ScMapEntry, ScSymbol};
        let entry = |k: &[u8], val: ScVal| ScMapEntry {
            key: ScVal::Symbol(ScSymbol::try_from(k.to_vec()).unwrap()),
            val,
        };
        ScVal::Map(Some(
            ScMap::try_from(vec![
                entry(b"amount", i128_scval(amount)),
                entry(b"authorized", ScVal::Bool(true)),
                entry(b"clawback", ScVal::Bool(false)),
            ])
            .unwrap(),
        ))
    }

    /// A `ContractData` `Balance(Address)` entry for `holder` under token/SAC
    /// contract `token`, carrying `val`.
    fn contract_data_balance_entry(token: u8, holder: u8, val: ScVal) -> LedgerEntry {
        use stellar_xdr::{ContractDataDurability, ContractDataEntry, ScSymbol, ScVec};
        let key = ScVal::Vec(Some(
            ScVec::try_from(vec![
                ScVal::Symbol(ScSymbol::try_from(b"Balance".to_vec()).unwrap()),
                ScVal::Address(contract_addr(holder)),
            ])
            .unwrap(),
        ));
        LedgerEntry {
            last_modified_ledger_seq: 100,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: contract_addr(token),
                key,
                durability: ContractDataDurability::Persistent,
                val,
            }),
            ext: LedgerEntryExt::V0,
        }
    }

    #[test]
    fn sac_contract_held_balance_telescopes_to_signed_delta() {
        // A contract's SAC balance 1000 -> 250: it SENT 750 → SacWrapped delta -750.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(contract_data_balance_entry(
                0x0A,
                0x0B,
                sac_balance_struct(1000),
            )),
            LedgerEntryChange::Updated(contract_data_balance_entry(
                0x0A,
                0x0B,
                sac_balance_struct(250),
            )),
        ]);
        let d = ledger_balance_deltas(&meta);
        let sac: Vec<_> = d
            .iter()
            .filter(|x| matches!(x.asset, LedgerAsset::SacWrapped(_)))
            .collect();
        assert_eq!(sac.len(), 1, "one SAC delta, got {d:?}");
        assert_eq!(sac[0].delta, -750);
    }

    #[test]
    fn bespoke_token_bare_i128_balance_telescopes_to_signed_delta() {
        // A bespoke token balance 500 -> 800: it RECEIVED 300 → Bespoke delta +300.
        let meta = meta_with_op_changes(vec![
            LedgerEntryChange::State(contract_data_balance_entry(0x1A, 0x1B, i128_scval(500))),
            LedgerEntryChange::Updated(contract_data_balance_entry(0x1A, 0x1B, i128_scval(800))),
        ]);
        let d = ledger_balance_deltas(&meta);
        let tok: Vec<_> = d
            .iter()
            .filter(|x| matches!(x.asset, LedgerAsset::Bespoke(_)))
            .collect();
        assert_eq!(tok.len(), 1, "one bespoke delta, got {d:?}");
        assert_eq!(tok[0].delta, 300);
    }

    #[test]
    fn contract_data_non_balance_key_is_ignored() {
        // A ContractData entry whose key is not `Balance(Address)` carries no balance.
        use stellar_xdr::{ContractDataDurability, ContractDataEntry, ScSymbol};
        let entry = LedgerEntry {
            last_modified_ledger_seq: 100,
            data: LedgerEntryData::ContractData(ContractDataEntry {
                ext: ExtensionPoint::V0,
                contract: contract_addr(0x2A),
                key: ScVal::Symbol(ScSymbol::try_from(b"Admin".to_vec()).unwrap()),
                durability: ContractDataDurability::Persistent,
                val: i128_scval(999),
            }),
            ext: LedgerEntryExt::V0,
        };
        let meta = meta_with_op_changes(vec![LedgerEntryChange::State(entry)]);
        assert!(ledger_balance_deltas(&meta).is_empty());
    }
}
