//! Operation → asset appearances (task 0359).
//!
//! The per-asset activity index is **pure presence**, modelled 1:1 on
//! `transaction_participants` for accounts: one row per (asset, transaction),
//! `asset_id`-leading so a per-asset page is a PK-prefix seek. This function
//! turns one operation into every asset DECLARED in its body — the "body" grain.
//! Two later grains complete it: **meta** (assets the body references only by id
//! — claimable balances, LP pools) and **result** (actual claim-atom crossings).
//!
//! ONE shared function on the shared parse path (live ingest and the archive
//! backfill both run it). Duplicates MAY repeat; the RMT sort key collapses them
//! to one (asset, tx) row and no read path can observe the difference. The match
//! is exhaustive (no `_`) so a new op type breaks compile here and its assets
//! are decided, never silently dropped.

use stellar_xdr::curr::{
    Asset, AssetCode, ChangeTrustAsset, LedgerKey, LiquidityPoolParameters, OperationBody,
    RevokeSponsorshipOp, TrustLineAsset,
};

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

/// Emit every asset an operation declares in its body. `op_source` is the
/// resolved operation source StrKey (the `AllowTrust` issuer). See the module
/// docs for the meta / result grains added by the following commits.
pub fn emit_asset_appearances(body: &OperationBody, op_source: &str) -> Vec<AssetRef> {
    let mut out: Vec<AssetRef> = Vec::new();

    match body {
        OperationBody::Payment(op) => out.push(asset_ref(&op.asset)),
        // Path payment declares a VARIABLE asset count in its body: source gives
        // `send_asset`, receiver gets `dest_asset`, and it routes THROUGH each
        // `path[]` hop (up to 5). All body-declared → all indexed, in route order.
        OperationBody::PathPaymentStrictReceive(op) => {
            out.push(asset_ref(&op.send_asset));
            out.extend(op.path.iter().map(asset_ref));
            out.push(asset_ref(&op.dest_asset));
        }
        OperationBody::PathPaymentStrictSend(op) => {
            out.push(asset_ref(&op.send_asset));
            out.extend(op.path.iter().map(asset_ref));
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
        OperationBody::ChangeTrust(op) => match &op.line {
            ChangeTrustAsset::Native => out.push(AssetRef::Native),
            ChangeTrustAsset::CreditAlphanum4(a) => {
                out.push(credit(a.asset_code.as_slice(), &a.issuer.0.to_string()))
            }
            ChangeTrustAsset::CreditAlphanum12(a) => {
                out.push(credit(a.asset_code.as_slice(), &a.issuer.0.to_string()))
            }
            // A pool-share trustline declares BOTH pool assets in its params.
            ChangeTrustAsset::PoolShare(LiquidityPoolParameters::LiquidityPoolConstantProduct(
                p,
            )) => {
                out.push(asset_ref(&p.asset_a));
                out.push(asset_ref(&p.asset_b));
            }
        },
        OperationBody::CreateClaimableBalance(op) => out.push(asset_ref(&op.asset)),
        OperationBody::Clawback(op) => out.push(asset_ref(&op.asset)),
        // Both move native XLM; the asset is implicit (not in the op body).
        OperationBody::CreateAccount(_) | OperationBody::AccountMerge(_) => {
            out.push(AssetRef::Native)
        }
        OperationBody::SetTrustLineFlags(op) => out.push(asset_ref(&op.asset)),
        // allow_trust: the code is in the body; the issuer is the OP SOURCE (the
        // account authorizing the trustline). Deprecated (SetTrustLineFlags) but
        // still recovered.
        OperationBody::AllowTrust(op) => {
            out.push(credit(asset_code_bytes(&op.asset), op_source))
        }
        // Sponsorship revoke of a TRUSTLINE declares its asset in the ledger key
        // (pool-share trustlines carry a pool id, not an asset → skipped).
        OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(
            LedgerKey::Trustline(k),
        )) => {
            if let Some(a) = trustline_asset_ref(&k.asset) {
                out.push(a);
            }
        }
        // Asset lives only in the op META (a balance/pool id in the body) — added
        // in the meta-grain commit. Listed here as body-empty for now.
        OperationBody::ClaimClaimableBalance(_)
        | OperationBody::ClawbackClaimableBalance(_)
        | OperationBody::LiquidityPoolDeposit(_)
        | OperationBody::LiquidityPoolWithdraw(_) => {}
        // No classic asset anywhere in these ops. Listed (never `_`) so a NEW
        // OperationBody variant breaks compile HERE. `RevokeSponsorship(_)`
        // catches every non-trustline revoke (offer / account / CB / pool / data
        // keys carry no single asset).
        OperationBody::SetOptions(_)
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
        Asset::CreditAlphanum4(a) => credit(a.asset_code.as_slice(), &a.issuer.0.to_string()),
        Asset::CreditAlphanum12(a) => credit(a.asset_code.as_slice(), &a.issuer.0.to_string()),
    }
}

/// A `TrustLineAsset` → [`AssetRef`]; `None` for a pool-share (a pool id, not an
/// asset).
fn trustline_asset_ref(asset: &TrustLineAsset) -> Option<AssetRef> {
    match asset {
        TrustLineAsset::Native => Some(AssetRef::Native),
        TrustLineAsset::CreditAlphanum4(a) => {
            Some(credit(a.asset_code.as_slice(), &a.issuer.0.to_string()))
        }
        TrustLineAsset::CreditAlphanum12(a) => {
            Some(credit(a.asset_code.as_slice(), &a.issuer.0.to_string()))
        }
        TrustLineAsset::PoolShare(_) => None,
    }
}

/// The raw code bytes of an `AssetCode` (allow_trust's issuer-less code).
fn asset_code_bytes(code: &AssetCode) -> &[u8] {
    match code {
        AssetCode::CreditAlphanum4(c) => c.as_slice(),
        AssetCode::CreditAlphanum12(c) => c.as_slice(),
    }
}

/// Build a credit [`AssetRef`] from raw code bytes + an issuer StrKey.
fn credit(code_bytes: &[u8], issuer: &str) -> AssetRef {
    AssetRef::Credit {
        code: asset_code_str(code_bytes),
        issuer: issuer.to_string(),
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
        AccountId, AllowTrustOp, AlphaNum4, AssetCode4, ChangeTrustOp, ClaimClaimableBalanceOp,
        ClaimableBalanceId, ClawbackOp, CreateAccountOp, CreateClaimableBalanceOp, Hash,
        LedgerKeyTrustLine, LiquidityPoolConstantProductParameters, ManageBuyOfferOp,
        ManageSellOfferOp, MuxedAccount, PathPaymentStrictSendOp, PaymentOp, Price, PublicKey,
        SetTrustLineFlagsOp, Uint256, VecM,
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
    fn cr(code: &str, issuer: &str) -> AssetRef {
        AssetRef::Credit {
            code: code.into(),
            issuer: issuer.into(),
        }
    }
    fn emit(body: &OperationBody) -> Vec<AssetRef> {
        emit_asset_appearances(body, "")
    }

    #[test]
    fn offers_emit_both_assets() {
        let sell = OperationBody::ManageSellOffer(ManageSellOfferOp {
            selling: Asset::Native,
            buying: xdr_credit(b"USDC", 0x01),
            amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        });
        assert_eq!(
            emit(&sell),
            vec![AssetRef::Native, cr("USDC", &issuer_str(0x01))]
        );
        let buy = OperationBody::ManageBuyOffer(ManageBuyOfferOp {
            selling: xdr_credit(b"AAA\0", 0x02),
            buying: Asset::Native,
            buy_amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        });
        assert_eq!(
            emit(&buy),
            vec![cr("AAA", &issuer_str(0x02)), AssetRef::Native]
        );
    }

    #[test]
    fn native_payment_keys_native_not_absence() {
        let body = OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: Asset::Native,
            amount: 10,
        });
        assert_eq!(emit(&body), vec![AssetRef::Native]);
    }

    #[test]
    fn path_payment_emits_send_hops_dest() {
        // Variable count: send + each path[] hop + dest (the up-to-7 case).
        let body = OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
            send_asset: Asset::Native,
            send_amount: 1_000,
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            dest_asset: xdr_credit(b"USDC", 0x01),
            dest_min: 900,
            path: vec![xdr_credit(b"EURT", 0x02), xdr_credit(b"BTC\0", 0x03)]
                .try_into()
                .unwrap(),
        });
        assert_eq!(
            emit(&body),
            vec![
                AssetRef::Native,
                cr("EURT", &issuer_str(0x02)),
                cr("BTC", &issuer_str(0x03)),
                cr("USDC", &issuer_str(0x01)),
            ]
        );
    }

    #[test]
    fn change_trust_pool_share_emits_both_pool_assets() {
        let body = OperationBody::ChangeTrust(ChangeTrustOp {
            line: ChangeTrustAsset::PoolShare(
                LiquidityPoolParameters::LiquidityPoolConstantProduct(
                    LiquidityPoolConstantProductParameters {
                        asset_a: Asset::Native,
                        asset_b: xdr_credit(b"USDC", 0x05),
                        fee: 30,
                    },
                ),
            ),
            limit: 100,
        });
        assert_eq!(
            emit(&body),
            vec![AssetRef::Native, cr("USDC", &issuer_str(0x05))]
        );
    }

    #[test]
    fn allow_trust_uses_op_source_as_issuer() {
        let body = OperationBody::AllowTrust(AllowTrustOp {
            trustor: acct(0x03),
            asset: AssetCode::CreditAlphanum4(AssetCode4(*b"USDC")),
            authorize: 1,
        });
        let src = issuer_str(0x09);
        assert_eq!(emit_asset_appearances(&body, &src), vec![cr("USDC", &src)]);
    }

    #[test]
    fn revoke_sponsorship_of_trustline_emits_its_asset() {
        let body = OperationBody::RevokeSponsorship(RevokeSponsorshipOp::LedgerEntry(
            LedgerKey::Trustline(LedgerKeyTrustLine {
                account_id: acct(0x03),
                asset: TrustLineAsset::CreditAlphanum4(AlphaNum4 {
                    asset_code: AssetCode4(*b"USDC"),
                    issuer: acct(0x01),
                }),
            }),
        ));
        assert_eq!(emit(&body), vec![cr("USDC", &issuer_str(0x01))]);
    }

    #[test]
    fn set_trustline_flags_and_clawback_emit_asset() {
        let flags = OperationBody::SetTrustLineFlags(SetTrustLineFlagsOp {
            trustor: acct(0x03),
            asset: xdr_credit(b"USDC", 0x01),
            clear_flags: 1,
            set_flags: 2,
        });
        assert_eq!(emit(&flags), vec![cr("USDC", &issuer_str(0x01))]);
        let cb = OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
            asset: xdr_credit(b"AQUA", 0x07),
            amount: 200,
            claimants: VecM::default(),
        });
        assert_eq!(emit(&cb), vec![cr("AQUA", &issuer_str(0x07))]);
        let claw = OperationBody::Clawback(ClawbackOp {
            asset: xdr_credit(b"USDC", 0x01),
            from: MuxedAccount::Ed25519(Uint256([0xAA; 32])),
            amount: 5,
        });
        assert_eq!(emit(&claw), vec![cr("USDC", &issuer_str(0x01))]);
    }

    #[test]
    fn create_account_and_merge_emit_native() {
        let create = OperationBody::CreateAccount(CreateAccountOp {
            destination: acct(0x04),
            starting_balance: 100,
        });
        let merge = OperationBody::AccountMerge(MuxedAccount::Ed25519(Uint256([0x05; 32])));
        assert_eq!(emit(&create), vec![AssetRef::Native]);
        assert_eq!(emit(&merge), vec![AssetRef::Native]);
    }

    #[test]
    fn meta_only_and_non_asset_ops_emit_nothing_from_body() {
        // Meta grain not wired yet — claim-CB / LP emit nothing from the body.
        let claim = OperationBody::ClaimClaimableBalance(ClaimClaimableBalanceOp {
            balance_id: ClaimableBalanceId::ClaimableBalanceIdTypeV0(Hash([0x11; 32])),
        });
        assert!(emit(&claim).is_empty());
        let bump = OperationBody::BumpSequence(stellar_xdr::curr::BumpSequenceOp {
            bump_to: stellar_xdr::curr::SequenceNumber(42),
        });
        assert!(emit(&bump).is_empty());
    }
}
