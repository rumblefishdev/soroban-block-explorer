//! Operation → account counterparties (task 0359 F-C / K1-5).
//!
//! The account-dimension twin of [`crate::asset_appearances`]: one typed emitter
//! turning an operation into every account it involves besides its own source,
//! staged into `transaction_participants`. Replaced the old string-`details`
//! extraction (a JSON round-trip with a silent `_` fallthrough); this reads the
//! typed `OperationBody` + `OperationResult` directly, exhaustively.
//!
//! Not here: the op source (staged separately), asset issuers (derived from
//! `asset_appearances`), and the non-op participant sources — fee-bump payer
//! ([`crate::envelope::envelope_fee_source`]), token-event from/to, invocation
//! caller — which staging unions in from their own XDR locations.

use crate::envelope::muxed_to_g_strkey;
use crate::operation::claim_atoms;
use stellar_xdr::{
    AccountId, ClaimAtom, Claimant, LedgerKey, OperationBody, OperationResult, PublicKey,
    RevokeSponsorshipOp,
};

/// Every account an operation involves besides its own source (task 0359 F-C /
/// K1-5) — the typed, single source of `transaction_participants` op-side
/// entries, replacing the old string-`details` participant extraction:
///
/// - simple destinations / targets: payment & path-payment & account-merge
///   destination, clawback `from`, allow-trust / set-trustline-flags `trustor`,
///   begin-sponsoring `sponsoredId`;
/// - crossed-offer SELLERS from the result's claim atoms (the maker is only in
///   the RESULT, never the body — unlike the asset grain, which drops the
///   redundant result). Success-gated upstream by `crate::operation::tx_op_results`;
/// - `CreateClaimableBalance` claimants (previously only their COUNT survived),
///   `SetOptions.inflationDest`, revoke-sponsorship targets.
///
/// Asset ISSUERS (also participants) are NOT here — staging derives them from
/// the op's `asset_appearances` (every credit leg already carries its issuer),
/// so this stays purely about account ROLES. The op source is registered
/// separately by staging.
///
/// Raw G-StrKeys in fixed XDR order (deterministic live vs backfill); the
/// staging participant set dedups. The body match is EXHAUSTIVE (no `_`) so a
/// new `OperationBody` variant breaks compile HERE and its counterparty is a
/// decision, never a silent drop — the same discipline as
/// [`crate::asset_appearances::emit_asset_appearances`].
pub(crate) fn extract_counterparties(
    body: &OperationBody,
    op_result: Option<&OperationResult>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match body {
        // Simple destination / target accounts (muxed addresses collapse to
        // their underlying G, as everywhere else).
        OperationBody::Payment(op) => out.push(muxed_to_g_strkey(&op.destination)),
        OperationBody::PathPaymentStrictReceive(op) => {
            out.push(muxed_to_g_strkey(&op.destination))
        }
        OperationBody::PathPaymentStrictSend(op) => out.push(muxed_to_g_strkey(&op.destination)),
        OperationBody::CreateAccount(op) => out.push(op.destination.0.to_string()),
        OperationBody::AccountMerge(dest) => out.push(muxed_to_g_strkey(dest)),
        OperationBody::Clawback(op) => out.push(muxed_to_g_strkey(&op.from)),
        OperationBody::AllowTrust(op) => out.push(op.trustor.0.to_string()),
        OperationBody::SetTrustLineFlags(op) => out.push(op.trustor.0.to_string()),
        OperationBody::BeginSponsoringFutureReserves(op) => {
            out.push(op.sponsored_id.0.to_string())
        }
        OperationBody::CreateClaimableBalance(op) => {
            for c in op.claimants.iter() {
                let Claimant::ClaimantTypeV0(v0) = c;
                out.push(v0.destination.0.to_string());
            }
        }
        OperationBody::SetOptions(op) => {
            if let Some(dest) = &op.inflation_dest {
                out.push(dest.0.to_string());
            }
        }
        OperationBody::RevokeSponsorship(op) => match op {
            RevokeSponsorshipOp::Signer(s) => out.push(s.account_id.0.to_string()),
            RevokeSponsorshipOp::LedgerEntry(key) => {
                if let Some(owner) = ledger_key_owner(key) {
                    out.push(owner);
                }
            }
        },
        // No account counterparty in the body. Offers carry their crossed seller
        // in the RESULT (handled below), not the body; claim/clawback-CB name the
        // claimant = the op source (registered separately); LP ops, trustline
        // changes and the Soroban / no-op family touch no extra account. Listed
        // exhaustively (never `_`): a NEW OperationBody variant must break compile
        // here so its counterparty is decided, not silently dropped.
        OperationBody::ManageSellOffer(_)
        | OperationBody::ManageBuyOffer(_)
        | OperationBody::CreatePassiveSellOffer(_)
        | OperationBody::ChangeTrust(_)
        | OperationBody::ClaimClaimableBalance(_)
        | OperationBody::ClawbackClaimableBalance(_)
        | OperationBody::LiquidityPoolDeposit(_)
        | OperationBody::LiquidityPoolWithdraw(_)
        | OperationBody::Inflation
        | OperationBody::ManageData(_)
        | OperationBody::BumpSequence(_)
        | OperationBody::EndSponsoringFutureReserves
        | OperationBody::InvokeHostFunction(_)
        | OperationBody::ExtendFootprintTtl(_)
        | OperationBody::RestoreFootprint(_) => {}
    }
    if let Some(res) = op_result {
        for atom in claim_atoms(res) {
            match atom {
                ClaimAtom::OrderBook(ob) => out.push(ob.seller_id.0.to_string()),
                // Pre-Protocol-11 atom: raw ed25519 seller, no wrapping AccountId.
                ClaimAtom::V0(v0) => out.push(
                    AccountId(PublicKey::PublicKeyTypeEd25519(v0.seller_ed25519.clone()))
                        .0
                        .to_string(),
                ),
                // A pool leg has no seller account (the AMM is the counterparty).
                ClaimAtom::LiquidityPool(_) => {}
            }
        }
    }
    out
}

/// The owning ACCOUNT of a sponsorship-revoked ledger entry, when it has one.
/// Claimable-balance / liquidity-pool / contract entries are keyed by a hash,
/// not an account, so they yield `None`.
fn ledger_key_owner(key: &LedgerKey) -> Option<String> {
    match key {
        LedgerKey::Account(k) => Some(k.account_id.0.to_string()),
        LedgerKey::Trustline(k) => Some(k.account_id.0.to_string()),
        LedgerKey::Offer(k) => Some(k.seller_id.0.to_string()),
        LedgerKey::Data(k) => Some(k.account_id.0.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        AllowTrustOp, Asset, AssetCode, AssetCode4, BeginSponsoringFutureReservesOp,
        ClaimLiquidityAtom, ClaimOfferAtom, ClaimOfferAtomV0, ClaimPredicate, ClaimableBalanceId,
        ClaimantV0, ClawbackOp, CreateAccountOp, CreateClaimableBalanceOp, Hash,
        LedgerKeyClaimableBalance, LedgerKeyTrustLine, ManageBuyOfferOp, ManageBuyOfferResult,
        ManageOfferSuccessResult, ManageOfferSuccessResultOffer, MuxedAccount, OperationResultTr,
        PaymentOp, PoolId, Price, RevokeSponsorshipOpSigner, SetOptionsOp, SetTrustLineFlagsOp,
        SignerKey, TrustLineAsset, Uint256,
    };

    /// The G-StrKey of an ed25519 account whose key bytes are all `b` — the
    /// canonical form `extract_counterparties` produces (`AccountId::to_string`).
    fn g_strkey(b: u8) -> String {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([b; 32])))
            .0
            .to_string()
    }

    /// An offer body — it names no account counterparty itself (the crossed
    /// seller lives in the RESULT), so it isolates the result-atom path.
    fn offer_body() -> OperationBody {
        OperationBody::ManageBuyOffer(ManageBuyOfferOp {
            selling: Asset::Native,
            buying: Asset::Native,
            buy_amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        })
    }

    // Result-atom builders (a local copy of the `operation` test helpers — the
    // seller-recovery tests need to wrap atoms in a success result).
    fn order_book_atom() -> ClaimAtom {
        ClaimAtom::OrderBook(ClaimOfferAtom {
            seller_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xCC; 32]))),
            offer_id: 7,
            asset_sold: Asset::Native,
            amount_sold: 10,
            asset_bought: Asset::Native,
            amount_bought: 10,
        })
    }

    fn lp_atom(pool_id: [u8; 32], amount_sold: i64, amount_bought: i64) -> ClaimAtom {
        ClaimAtom::LiquidityPool(ClaimLiquidityAtom {
            liquidity_pool_id: PoolId(Hash(pool_id)),
            asset_sold: Asset::Native,
            amount_sold,
            asset_bought: Asset::Native,
            amount_bought,
        })
    }

    fn manage_buy_offer_result(claimed: Vec<ClaimAtom>) -> OperationResult {
        OperationResult::OpInner(OperationResultTr::ManageBuyOffer(
            ManageBuyOfferResult::Success(ManageOfferSuccessResult {
                offers_claimed: claimed.try_into().unwrap(),
                offer: ManageOfferSuccessResultOffer::Deleted,
            }),
        ))
    }

    #[test]
    fn simple_destination_counterparties() {
        let payment = OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: Asset::Native,
            amount: 10,
        });
        assert_eq!(extract_counterparties(&payment, None), vec![g_strkey(0xBB)]);

        let create = OperationBody::CreateAccount(CreateAccountOp {
            destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xC1; 32]))),
            starting_balance: 100,
        });
        assert_eq!(extract_counterparties(&create, None), vec![g_strkey(0xC1)]);

        let merge = OperationBody::AccountMerge(MuxedAccount::Ed25519(Uint256([0xC2; 32])));
        assert_eq!(extract_counterparties(&merge, None), vec![g_strkey(0xC2)]);

        let clawback = OperationBody::Clawback(ClawbackOp {
            asset: Asset::Native,
            from: MuxedAccount::Ed25519(Uint256([0xC3; 32])),
            amount: 5,
        });
        assert_eq!(extract_counterparties(&clawback, None), vec![g_strkey(0xC3)]);

        let allow = OperationBody::AllowTrust(AllowTrustOp {
            trustor: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xC4; 32]))),
            asset: AssetCode::CreditAlphanum4(AssetCode4(*b"USDC")),
            authorize: 1,
        });
        assert_eq!(extract_counterparties(&allow, None), vec![g_strkey(0xC4)]);

        let flags = OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
            trustor: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xC5; 32]))),
            asset: Asset::Native,
            clear_flags: 0,
            set_flags: 1,
        });
        assert_eq!(extract_counterparties(&flags, None), vec![g_strkey(0xC5)]);

        let sponsor =
            OperationBody::BeginSponsoringFutureReserves(BeginSponsoringFutureReservesOp {
                sponsored_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xC6; 32]))),
            });
        assert_eq!(extract_counterparties(&sponsor, None), vec![g_strkey(0xC6)]);
    }

    #[test]
    fn create_claimable_balance_counterparties_are_claimants() {
        let claimant = |b: u8| {
            Claimant::ClaimantTypeV0(ClaimantV0 {
                destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([b; 32]))),
                predicate: ClaimPredicate::Unconditional,
            })
        };
        let body = OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
            asset: Asset::Native,
            amount: 100,
            claimants: vec![claimant(0x01), claimant(0x02)].try_into().unwrap(),
        });
        assert_eq!(
            extract_counterparties(&body, None),
            vec![g_strkey(0x01), g_strkey(0x02)]
        );
    }

    #[test]
    fn set_options_counterparty_is_inflation_dest() {
        let opts = |inflation_dest| {
            OperationBody::SetOptions(SetOptionsOp {
                inflation_dest,
                clear_flags: None,
                set_flags: None,
                master_weight: None,
                low_threshold: None,
                med_threshold: None,
                high_threshold: None,
                signer: None,
                home_domain: None,
            })
        };
        let dest = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x07; 32])));
        assert_eq!(
            extract_counterparties(&opts(Some(dest)), None),
            vec![g_strkey(0x07)]
        );
        assert!(extract_counterparties(&opts(None), None).is_empty());
    }

    #[test]
    fn revoke_sponsorship_counterparty_signer_and_entry_owner() {
        // Signer variant → the signer's account.
        let signer = OperationBody::RevokeSponsorship(RevokeSponsorshipOp::Signer(
            RevokeSponsorshipOpSigner {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x03; 32]))),
                signer_key: SignerKey::Ed25519(Uint256([0x09; 32])),
            },
        ));
        assert_eq!(extract_counterparties(&signer, None), vec![g_strkey(0x03)]);

        // LedgerEntry(Trustline) → the trustline's owner account.
        let entry = OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(
            LedgerKey::Trustline(LedgerKeyTrustLine {
                account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x04; 32]))),
                asset: TrustLineAsset::Native,
            }),
        ));
        assert_eq!(extract_counterparties(&entry, None), vec![g_strkey(0x04)]);

        // LedgerEntry(ClaimableBalance) → keyed by a hash, no owning account.
        let cb = OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(
            LedgerKey::ClaimableBalance(LedgerKeyClaimableBalance {
                balance_id: ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash([0x05; 32])),
            }),
        ));
        assert!(extract_counterparties(&cb, None).is_empty());
    }

    #[test]
    fn crossed_offer_seller_recovered_from_result_atoms() {
        let body = offer_body();
        // No result → an offer body alone names no counterparty.
        assert!(extract_counterparties(&body, None).is_empty());

        // OrderBook atom → its seller_id (0xCC, from `order_book_atom`).
        let ob = manage_buy_offer_result(vec![order_book_atom()]);
        assert_eq!(extract_counterparties(&body, Some(&ob)), vec![g_strkey(0xCC)]);

        // LiquidityPool atom → no seller account (the AMM is the counterparty).
        let lp = manage_buy_offer_result(vec![lp_atom([0x33; 32], 1, 1)]);
        assert!(extract_counterparties(&body, Some(&lp)).is_empty());
    }

    #[test]
    fn v0_claim_atom_seller_recovered() {
        let body = offer_body();
        let v0 = ClaimAtom::V0(ClaimOfferAtomV0 {
            seller_ed25519: Uint256([0x0A; 32]),
            offer_id: 1,
            asset_sold: Asset::Native,
            amount_sold: 5,
            asset_bought: Asset::Native,
            amount_bought: 5,
        });
        let res = manage_buy_offer_result(vec![v0]);
        assert_eq!(extract_counterparties(&body, Some(&res)), vec![g_strkey(0x0A)]);
    }
}
