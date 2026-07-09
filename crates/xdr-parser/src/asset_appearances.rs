//! Operation → asset appearances (task 0359).
//!
//! The per-asset activity index is **pure presence**, modelled 1:1 on
//! `transaction_participants` for accounts: one row per (asset, transaction),
//! `asset_id`-leading so a per-asset page is a PK-prefix seek. This function
//! turns one operation into the assets it DECLARES in its body — the same
//! "declared participants" grain `transaction_participants` uses. ONE shared
//! function on the shared parse path (live ingest and the archive backfill both
//! run it). Duplicates MAY repeat; the RMT sort key collapses them to one
//! (asset, tx) row and no read path can observe the difference.
//!
//! Scope (minimal, karolkow 2026-07-09): body-declared assets only. Assets that
//! live only in the result claim atoms (path hops, offer crossings) or in the
//! operation meta (`claim_claimable_balance`, `liquidity_pool_deposit/withdraw`
//! carry only an id) are NOT recovered here — that is deferred completeness, the
//! same class as the crossed-offer counterparties still missing from
//! `transaction_participants`. Added by the completeness follow-up, not before.

use stellar_xdr::curr::{Asset, ChangeTrustAsset, OperationBody};

/// An asset as it appears in an operation, before surrogate-hashing.
///
/// The numeric `asset_id` surrogate is computed later, in the persistence layer
/// (`ids::asset_id`), which needs the already-hashed issuer surrogate. This
/// parse-side type stays in StrKey terms: `code` + issuer G-StrKey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRef {
    Native,
    Credit { code: String, issuer: String },
}

/// Emit the assets declared in a single operation's body, in fixed XDR slot
/// order. See the module docs for the (deliberately minimal) scope.
pub fn emit_asset_appearances(body: &OperationBody) -> Vec<AssetRef> {
    let mut out: Vec<AssetRef> = Vec::new();
    match body {
        OperationBody::Payment(op) => out.push(asset_ref(&op.asset)),
        OperationBody::PathPaymentStrictReceive(op) => {
            out.push(asset_ref(&op.send_asset));
            out.push(asset_ref(&op.dest_asset));
        }
        OperationBody::PathPaymentStrictSend(op) => {
            out.push(asset_ref(&op.send_asset));
            out.push(asset_ref(&op.dest_asset));
        }
        OperationBody::ManageSellOffer(op) => {
            out.push(asset_ref(&op.selling));
            out.push(asset_ref(&op.buying));
        }
        OperationBody::ManageBuyOffer(op) => {
            out.push(asset_ref(&op.selling));
            out.push(asset_ref(&op.buying));
        }
        OperationBody::CreatePassiveSellOffer(op) => {
            out.push(asset_ref(&op.selling));
            out.push(asset_ref(&op.buying));
        }
        // A trustline to a pool SHARE is a pool-dimension event, not activity of
        // asset A or B — PoolShare emits nothing here.
        OperationBody::ChangeTrust(op) => match &op.line {
            ChangeTrustAsset::Native => out.push(AssetRef::Native),
            ChangeTrustAsset::CreditAlphanum4(a) => out.push(AssetRef::Credit {
                code: asset_code_str(a.asset_code.as_slice()),
                issuer: a.issuer.0.to_string(),
            }),
            ChangeTrustAsset::CreditAlphanum12(a) => out.push(AssetRef::Credit {
                code: asset_code_str(a.asset_code.as_slice()),
                issuer: a.issuer.0.to_string(),
            }),
            ChangeTrustAsset::PoolShare(_) => {}
        },
        OperationBody::CreateClaimableBalance(op) => out.push(asset_ref(&op.asset)),
        OperationBody::Clawback(op) => out.push(asset_ref(&op.asset)),
        // Both move native XLM; the asset is implicit (not in the op body).
        OperationBody::CreateAccount(_) | OperationBody::AccountMerge(_) => {
            out.push(AssetRef::Native)
        }
        OperationBody::SetTrustLineFlags(op) => out.push(asset_ref(&op.asset)),
        // Asset lives only in the operation meta (a balance/pool id in the body),
        // deferred completeness — see module docs.
        OperationBody::ClaimClaimableBalance(_)
        | OperationBody::ClawbackClaimableBalance(_)
        | OperationBody::LiquidityPoolDeposit(_)
        | OperationBody::LiquidityPoolWithdraw(_) => {}
        // No classic asset in the body — listed (never `_`) so a NEW
        // OperationBody variant breaks compile HERE and its asset appearance is
        // decided, never silently dropped (the exact gap that let offers store
        // zero assets).
        OperationBody::AllowTrust(_)
        | OperationBody::SetOptions(_)
        | OperationBody::Inflation
        | OperationBody::ManageData(_)
        | OperationBody::BumpSequence(_)
        | OperationBody::BeginSponsoringFutureReserves(_)
        | OperationBody::EndSponsoringFutureReserves
        | OperationBody::RevokeSponsorship(_)
        // Soroban token flow surfaces via events, not the classic op body.
        | OperationBody::InvokeHostFunction(_)
        | OperationBody::ExtendFootprintTtl(_)
        | OperationBody::RestoreFootprint(_) => {}
    }
    out
}

/// Convert a raw XDR `Asset` to an [`AssetRef`].
fn asset_ref(asset: &Asset) -> AssetRef {
    match asset {
        Asset::Native => AssetRef::Native,
        Asset::CreditAlphanum4(a) => AssetRef::Credit {
            code: asset_code_str(a.asset_code.as_slice()),
            issuer: a.issuer.0.to_string(),
        },
        Asset::CreditAlphanum12(a) => AssetRef::Credit {
            code: asset_code_str(a.asset_code.as_slice()),
            issuer: a.issuer.0.to_string(),
        },
    }
}

/// Normalize a 4/12-byte asset code the SAME way `operation::format_asset` does
/// (strict UTF-8, `<invalid>` fallback, trim trailing NULs) so the derived
/// `asset_id` surrogate matches the `assets` table's key for the same asset.
fn asset_code_str(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .unwrap_or("<invalid>")
        .trim_end_matches('\0')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::curr::{
        AccountId, AlphaNum4, AssetCode4, ChangeTrustOp, ClaimClaimableBalanceOp,
        ClaimableBalanceId, ClawbackOp, CreateAccountOp, CreateClaimableBalanceOp, Hash,
        LiquidityPoolDepositOp, ManageBuyOfferOp, ManageSellOfferOp, MuxedAccount,
        PathPaymentStrictSendOp, PaymentOp, PoolId, Price, PublicKey, SetTrustLineFlagsOp, Uint256,
        VecM,
    };

    fn acct(b: u8) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([b; 32])))
    }
    fn issuer_str(b: u8) -> String {
        acct(b).0.to_string()
    }
    fn xdr_credit(code: &[u8; 4], issuer: u8) -> Asset {
        Asset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(*code),
            issuer: acct(issuer),
        })
    }
    fn credit(code: &str, issuer: &str) -> AssetRef {
        AssetRef::Credit {
            code: code.into(),
            issuer: issuer.into(),
        }
    }

    #[test]
    fn sell_offer_emits_both_assets() {
        // Offers store ZERO assets in the legacy single slot (the flagship bug).
        let body = OperationBody::ManageSellOffer(ManageSellOfferOp {
            selling: Asset::Native,
            buying: xdr_credit(b"USDC", 0x01),
            amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![AssetRef::Native, credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn buy_offer_emits_both_assets() {
        let body = OperationBody::ManageBuyOffer(ManageBuyOfferOp {
            selling: xdr_credit(b"AAA\0", 0x02),
            buying: Asset::Native,
            buy_amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![credit("AAA", &issuer_str(0x02)), AssetRef::Native]
        );
    }

    #[test]
    fn native_payment_keys_native_not_absence() {
        let body = OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: Asset::Native,
            amount: 10,
        });
        assert_eq!(emit_asset_appearances(&body), vec![AssetRef::Native]);
    }

    #[test]
    fn credit_payment_emits_single_asset() {
        let body = OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: xdr_credit(b"USDC", 0x01),
            amount: 100,
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn path_payment_emits_both_endpoints() {
        let body = OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
            send_asset: Asset::Native,
            send_amount: 1_000,
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            dest_asset: xdr_credit(b"USDC", 0x01),
            dest_min: 900,
            path: VecM::default(),
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![AssetRef::Native, credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn change_trust_emits_asset_pool_share_nothing() {
        let body = OperationBody::ChangeTrust(ChangeTrustOp {
            line: ChangeTrustAsset::CreditAlphanum4(AlphaNum4 {
                asset_code: AssetCode4(*b"USDC"),
                issuer: acct(0x01),
            }),
            limit: 100,
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn create_claimable_balance_and_clawback_emit_body_asset() {
        let cb = OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
            asset: xdr_credit(b"AQUA", 0x07),
            amount: 200,
            claimants: VecM::default(),
        });
        assert_eq!(
            emit_asset_appearances(&cb),
            vec![credit("AQUA", &issuer_str(0x07))]
        );
        let cb2 = OperationBody::Clawback(ClawbackOp {
            asset: xdr_credit(b"USDC", 0x01),
            from: MuxedAccount::Ed25519(Uint256([0xAA; 32])),
            amount: 5,
        });
        assert_eq!(
            emit_asset_appearances(&cb2),
            vec![credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn set_trustline_flags_emits_asset() {
        let body = OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
            trustor: acct(0x03),
            asset: xdr_credit(b"USDC", 0x01),
            clear_flags: 1,
            set_flags: 2,
        });
        assert_eq!(
            emit_asset_appearances(&body),
            vec![credit("USDC", &issuer_str(0x01))]
        );
    }

    #[test]
    fn create_account_and_merge_emit_native() {
        let create = OperationBody::CreateAccount(CreateAccountOp {
            destination: acct(0x04),
            starting_balance: 100,
        });
        let merge = OperationBody::AccountMerge(MuxedAccount::Ed25519(Uint256([0x05; 32])));
        assert_eq!(emit_asset_appearances(&create), vec![AssetRef::Native]);
        assert_eq!(emit_asset_appearances(&merge), vec![AssetRef::Native]);
    }

    #[test]
    fn meta_only_and_non_asset_ops_emit_nothing() {
        // Deferred completeness: asset lives in meta, not the body.
        let claim = OperationBody::ClaimClaimableBalance(ClaimClaimableBalanceOp {
            balance_id: ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash([0x11; 32])),
        });
        assert!(emit_asset_appearances(&claim).is_empty());
        let lp = OperationBody::LiquidityPoolDeposit(LiquidityPoolDepositOp {
            liquidity_pool_id: PoolId(Hash([0x44; 32])),
            max_amount_a: 10,
            max_amount_b: 20,
            min_price: Price { n: 1, d: 2 },
            max_price: Price { n: 2, d: 1 },
        });
        assert!(emit_asset_appearances(&lp).is_empty());
        // bump_sequence touches no asset — recorded N/A, not a silent drop.
        let bump = OperationBody::BumpSequence(stellar_xdr::curr::BumpSequenceOp {
            bump_to: stellar_xdr::curr::SequenceNumber(42),
        });
        assert!(emit_asset_appearances(&bump).is_empty());
    }
}
