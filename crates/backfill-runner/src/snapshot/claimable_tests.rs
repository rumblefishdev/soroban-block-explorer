use super::*;
use crate::snapshot::archive::SnapshotRecord;
use db_clickhouse::persist::ids;
use stellar_xdr::{
    AccountId, AlphaNum4, Asset, AssetCode4, ClaimPredicate, ClaimableBalanceEntry,
    ClaimableBalanceEntryExt, ClaimableBalanceId, Claimant, ClaimantV0, Hash, LedgerEntry,
    LedgerEntryData, LedgerEntryExt, LedgerKey, LedgerKeyClaimableBalance, PublicKey, Uint256,
};

/// The AVLX balance claimed in mainnet tx `23273fda…c7b8` (ledger 64,438,024).
const AVLX_BALANCE_HEX: &str = "6e18eca78c9ac43059e5c5d8f9789c14f8f8152c1ae5b0fcde85be461f870acc";
/// Production `asset_transfers.from_id` for that claim.
const AVLX_BALANCE_SURROGATE: i64 = 1_280_410_223_283_636_341;
const AVLX_ISSUER: &str = "GDKHHVS4SBVOLDGZNF2CVYW3TM7LRHK3NCVZE22MUEOYMQSMDQD2AVLX";

fn balance_id(hex_id: &str) -> ClaimableBalanceId {
    ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash(
        hex::decode(hex_id).unwrap().try_into().unwrap(),
    ))
}

fn avlx() -> Asset {
    let issuer: stellar_strkey::ed25519::PublicKey = AVLX_ISSUER.parse().unwrap();
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"AVLX"),
        issuer: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(issuer.0))),
    })
}

fn live(hex_id: &str, asset: Asset, amount: i64, ledger: u32) -> SnapshotRecord {
    SnapshotRecord::Live(Box::new(LedgerEntry {
        last_modified_ledger_seq: ledger,
        data: LedgerEntryData::ClaimableBalance(ClaimableBalanceEntry {
            balance_id: balance_id(hex_id),
            claimants: vec![Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([7; 32]))),
                predicate: ClaimPredicate::Unconditional,
            })]
            .try_into()
            .unwrap(),
            asset,
            amount,
            ext: ClaimableBalanceEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    }))
}

fn dead(hex_id: &str) -> SnapshotRecord {
    SnapshotRecord::Dead(Box::new(LedgerKey::ClaimableBalance(
        LedgerKeyClaimableBalance {
            balance_id: balance_id(hex_id),
        },
    )))
}

fn our_row(holder_id: i64, asset_id: i64) -> OurRow {
    OurRow {
        holder_id,
        asset_id,
        amount: 91_000,
        last_updated_ledger: 64_000_000,
        closed_at_ledger: 0,
    }
}

#[test]
fn snapshot_keys_a_balance_the_way_the_writer_and_asset_transfers_do() {
    let mut state = NetworkState::default();
    state.absorb_record(&live(AVLX_BALANCE_HEX, avlx(), 91_000, 64_000_000));

    let asset_id = ids::credit_asset_id("AVLX", AVLX_ISSUER);
    let e = state.claimable_balances[&AVLX_BALANCE_SURROGATE];
    assert!(e.live);
    assert_eq!((e.balance, e.ledger), (91_000, 64_000_000));
    assert_eq!(state.claimable_assets[&AVLX_BALANCE_SURROGATE], asset_id);
    // Registered so the seed can stub an asset we have never seen.
    assert_eq!(
        state.asset_registry[&asset_id],
        ("AVLX".to_string(), AVLX_ISSUER.to_string())
    );
}

#[test]
fn a_removal_seen_first_is_not_resurrected_by_an_older_live_record() {
    let mut state = NetworkState::default();
    state.absorb_record(&dead(AVLX_BALANCE_HEX));
    state.absorb_record(&live(AVLX_BALANCE_HEX, avlx(), 91_000, 60_000_000));

    assert!(!state.claimable_balances[&AVLX_BALANCE_SURROGATE].live);
    assert!(!state.claimable_assets.contains_key(&AVLX_BALANCE_SURROGATE));
    assert_eq!(state.superseded, 1);
    assert_eq!(state.live_claimable_balances(), 0);
}

#[test]
fn claim_matches_only_the_same_asset() {
    let mut state = NetworkState::default();
    state.absorb_record(&live(AVLX_BALANCE_HEX, avlx(), 91_000, 64_000_000));
    let asset_id = ids::credit_asset_id("AVLX", AVLX_ISSUER);

    // A different asset under the same balance: left unmatched, so our row
    // closes as gone and the network's is inserted under its real asset.
    assert!(
        claim(
            &mut state,
            &our_row(AVLX_BALANCE_SURROGATE, ids::NATIVE_ASSET_ID)
        )
        .is_none()
    );
    assert!(!state.claimable_balances[&AVLX_BALANCE_SURROGATE].matched);

    assert!(claim(&mut state, &our_row(AVLX_BALANCE_SURROGATE, asset_id)).is_some());
    assert!(state.claimable_balances[&AVLX_BALANCE_SURROGATE].matched);
}

#[test]
fn a_dead_balance_is_claimed_whatever_asset_our_row_carries() {
    let mut state = NetworkState::default();
    state.absorb_record(&dead(AVLX_BALANCE_HEX));
    let net = claim(&mut state, &our_row(AVLX_BALANCE_SURROGATE, 42)).expect("claimed");
    assert!(!net.live);
    // Our positive row against a removed balance is a ghost, closed at the
    // checkpoint.
    let row = our_row(AVLX_BALANCE_SURROGATE, 42);
    assert_eq!(
        verdict::verdict(&row, Some(&net), 64_400_000),
        Verdict::Ghost
    );
}

#[test]
fn writer_coverage_requires_a_claim_at_or_before_the_checkpoint() {
    assert!(writer_coverage(None, 64_400_000).is_err());
    assert!(writer_coverage(Some(64_400_001), 64_400_000).is_err());
    assert!(writer_coverage(Some(64_400_000), 64_400_000).is_ok());
    assert!(writer_coverage(Some(64_300_000), 64_400_000).is_ok());
}
