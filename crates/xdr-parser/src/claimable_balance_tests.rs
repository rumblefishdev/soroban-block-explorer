use super::*;
use crate::extract_ledger_entry_changes;
use stellar_xdr::{
    AccountId, AlphaNum4, Asset, AssetCode4, ClaimPredicate, ClaimableBalanceEntry,
    ClaimableBalanceEntryExt, Claimant, ClaimantV0, ExtensionPoint, LedgerEntry, LedgerEntryChange,
    LedgerEntryChanges, LedgerEntryData, LedgerEntryExt, LedgerKey, LedgerKeyClaimableBalance,
    OperationMeta, PublicKey, TransactionMeta, TransactionMetaV3, Uint256, VecM,
};

const LEDGER: u32 = 64_400_000;

fn account(byte: u8) -> AccountId {
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32])))
}

fn usdc() -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USDC"),
        issuer: account(0x11),
    })
}

fn id(byte: u8) -> ClaimableBalanceId {
    ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash([byte; 32]))
}

fn entry(byte: u8, asset: Asset, amount: i64) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: LEDGER,
        data: LedgerEntryData::ClaimableBalance(ClaimableBalanceEntry {
            balance_id: id(byte),
            claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: account(0x22),
                predicate: ClaimPredicate::Unconditional,
            })]
            .try_into()
            .unwrap(),
            asset,
            amount,
            ext: ClaimableBalanceEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    }
}

fn removed(byte: u8) -> LedgerEntryChange {
    LedgerEntryChange::Removed(LedgerKey::ClaimableBalance(LedgerKeyClaimableBalance {
        balance_id: id(byte),
    }))
}

/// Goes through the REAL change encoder, so a change to how a claimable
/// balance is rendered breaks these tests instead of silently matching nothing.
fn extract(ops: Vec<Vec<LedgerEntryChange>>) -> Vec<ExtractedClaimableBalance> {
    let meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: ops
            .into_iter()
            .map(|changes| OperationMeta {
                changes: changes.try_into().unwrap(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });
    extract_claimable_balances(&extract_ledger_entry_changes(
        &meta,
        &"ab".repeat(32),
        LEDGER,
        1_789_000_000,
    ))
}

fn strkey(byte: u8) -> String {
    ScAddress::ClaimableBalance(id(byte)).to_string()
}

fn usdc_asset() -> ClaimableBalanceAsset {
    ClaimableBalanceAsset::Credit {
        code: "USDC".to_string(),
        issuer: account(0x11).to_string(),
    }
}

#[test]
fn created_balance_is_a_live_holding() {
    let got = extract(vec![vec![LedgerEntryChange::Created(entry(
        0xB1,
        usdc(),
        5_000_000,
    ))]]);
    assert_eq!(
        got,
        vec![ExtractedClaimableBalance {
            balance_id: strkey(0xB1),
            asset: usdc_asset(),
            amount: 5_000_000,
            ledger_sequence: LEDGER,
            closed: false,
        }]
    );
    assert!(got[0].balance_id.starts_with('B'));
}

#[test]
fn claim_takes_the_asset_from_the_pre_image() {
    // Created in an earlier ledger, so this transaction only carries the
    // protocol's `state` pre-image and the key-only removal.
    let got = extract(vec![vec![
        LedgerEntryChange::State(entry(0xB2, Asset::Native, 9_000)),
        removed(0xB2),
    ]]);
    assert_eq!(
        got,
        vec![ExtractedClaimableBalance {
            balance_id: strkey(0xB2),
            asset: ClaimableBalanceAsset::Native,
            amount: 0,
            ledger_sequence: LEDGER,
            closed: true,
        }]
    );
}

#[test]
fn create_and_claim_in_one_transaction_leave_one_tombstone() {
    // Both changes share the ledger, so as two rows they would tie on the
    // ReplacingMergeTree version and the survivor would be insert order.
    let got = extract(vec![
        vec![LedgerEntryChange::Created(entry(0xB3, usdc(), 700))],
        vec![
            LedgerEntryChange::State(entry(0xB3, usdc(), 700)),
            removed(0xB3),
        ],
    ]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].asset, usdc_asset());
    assert_eq!((got[0].amount, got[0].closed), (0, true));
}

#[test]
fn removal_without_any_asset_is_dropped_not_guessed() {
    assert!(extract(vec![vec![removed(0xB4)]]).is_empty());
}

#[test]
fn other_entry_types_are_ignored() {
    let account_entry = LedgerEntry {
        last_modified_ledger_seq: LEDGER,
        data: LedgerEntryData::Account(stellar_xdr::AccountEntry {
            account_id: account(0x33),
            balance: 1,
            seq_num: stellar_xdr::SequenceNumber(1),
            num_sub_entries: 0,
            inflation_dest: None,
            flags: 0,
            home_domain: stellar_xdr::String32::default(),
            thresholds: stellar_xdr::Thresholds([1, 0, 0, 0]),
            signers: VecM::default(),
            ext: stellar_xdr::AccountEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    };
    assert!(extract(vec![vec![LedgerEntryChange::Created(account_entry)]]).is_empty());
}
