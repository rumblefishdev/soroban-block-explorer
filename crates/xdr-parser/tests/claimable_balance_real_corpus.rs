//! Claimable balances on real mainnet transactions (task 0210). Each fixture is
//! a captured `resultMetaXdr` (Soroban RPC `getTransaction`).
//!
//! - `claimable_balance` — the create already pinned by
//!   `net_settled_real_corpus` against an independent stellar-CLI decode.
//! - `claimable_balance_claim` — tx `23273fda…c7b8`, ledger 64,438,024, three
//!   claims. Cross-checked on 2026-09-15 against production `asset_transfers`,
//!   which decodes the same claims from EVENTS rather than entries: operations
//!   1, 4 and 5 move AVLX 91,000, MAKER 5,000,000,000 and MAKER 10,000,000,000
//!   stroops out of `B…` balances.
//!
//! The claim fixture is what the unit tests cannot show: that on chain the
//! key-only `removed` really is preceded by a `state` pre-image carrying the
//! asset. Without it no tombstone could be keyed.

use base64::Engine;
use stellar_xdr::{Limits, ReadXdr, TransactionMeta};
use xdr_parser::claimable_balance::{
    ClaimableBalanceAsset, ExtractedClaimableBalance, extract_claimable_balances,
};
use xdr_parser::extract_ledger_entry_changes;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/corpus/");

fn holdings(fixture: &str, ledger: u32) -> Vec<ExtractedClaimableBalance> {
    let b64 = std::fs::read_to_string(format!("{DIR}{fixture}.b64")).expect("fixture");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .expect("base64");
    let meta = TransactionMeta::from_xdr(bytes, Limits::none()).expect("decode meta");
    extract_claimable_balances(&extract_ledger_entry_changes(&meta, "tx", ledger, 0))
}

fn credit(code: &str, issuer: &str) -> ClaimableBalanceAsset {
    ClaimableBalanceAsset::Credit {
        code: code.to_string(),
        issuer: issuer.to_string(),
    }
}

#[test]
fn create_on_mainnet_is_a_live_holding() {
    assert_eq!(
        holdings("claimable_balance", 1),
        vec![ExtractedClaimableBalance {
            balance_id: "BAALKCO6IRHEUHHXEXBQNI6ETZ3XPP5VMCJ475J5BQTIYAJ22VXMZPWH4M".to_string(),
            asset: credit(
                "dSTARDUST",
                "GCUXCKROP3B353YX2CGDAFB4ADKLGPFG4LLFU7CLHSELVOP4LAXDIYF7"
            ),
            amount: 52_222_151_490_373,
            ledger_sequence: 1,
            closed: false,
        }]
    );
}

#[test]
fn claims_on_mainnet_are_tombstones_keyed_by_their_pre_image_asset() {
    const MAKER_ISSUER: &str = "GAKMTXKEF4DZVWRNCK5NTNEGVIIV3EQCIP4DDF5HG3TJ4NNP6KIS4OCF";
    let tombstone = |balance_id: &str, asset| ExtractedClaimableBalance {
        balance_id: balance_id.to_string(),
        asset,
        amount: 0,
        ledger_sequence: 64_438_024,
        closed: true,
    };
    assert_eq!(
        holdings("claimable_balance_claim", 64_438_024),
        vec![
            tombstone(
                "BAAG4GHMU6GJVRBQLHS4LWHZPCOBJ6HYCUWBVZNQ7TPILPSGD6DQVTCBEM",
                credit(
                    "AVLX",
                    "GDKHHVS4SBVOLDGZNF2CVYW3TM7LRHK3NCVZE22MUEOYMQSMDQD2AVLX"
                ),
            ),
            tombstone(
                "BAAA6SPVX4Y3QQV5NRSNT3JBCMM3DHUMBTOZHQMSFYVREXO46WILGOWCRU",
                credit("MAKER", MAKER_ISSUER),
            ),
            tombstone(
                "BAABZ74TXFL4JG33XARLHVNF4CQXJGEL5L7VUKYHN7IYNEKC6K2TX6FJX4",
                credit("MAKER", MAKER_ISSUER),
            ),
        ]
    );
}
