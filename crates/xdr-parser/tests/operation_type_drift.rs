//! Drift guard for `domain::OperationType` (task 0431).
//!
//! `domain::OperationType` is a hand-maintained mirror of
//! `stellar_xdr::OperationType`: `domain` stays dependency-light and the mirror
//! carries serde/utoipa derives the XDR type lacks. Until now nothing enforced
//! the "mirror" claim — the enum's own doc comment asserts the discriminants
//! match "byte-for-byte", but `domain` does not even import `stellar_xdr`.
//!
//! A silently-drifting hand copy of an XDR structure is exactly the class of bug
//! behind task 0430. This test pins the mirror three ways:
//!
//!   * the exhaustive `match` in [`mirror`] makes an *added* upstream variant a
//!     COMPILE error here (protocol bump adds an operation → build fails);
//!   * a *renumbered* discriminant is a TEST failure (the `assert_eq!` loop);
//!   * a *removed* variant is a TEST failure (the terminal count check).
//!
//! It needs no database, so it runs unconditionally in CI (cf. task 0406's
//! silent-skip trap).

use domain::OperationType as Ours;
use stellar_xdr::OperationType as Xdr;

/// Total mapping from every `stellar_xdr` operation variant to our mirror.
///
/// Exhaustive by design: if `stellar-xdr` gains a variant on a protocol bump,
/// this stops compiling until the mirror is updated. Arms map by *name*; the
/// test asserts the *discriminants* agree.
fn mirror(x: Xdr) -> Ours {
    match x {
        Xdr::CreateAccount => Ours::CreateAccount,
        Xdr::Payment => Ours::Payment,
        Xdr::PathPaymentStrictReceive => Ours::PathPaymentStrictReceive,
        Xdr::ManageSellOffer => Ours::ManageSellOffer,
        Xdr::CreatePassiveSellOffer => Ours::CreatePassiveSellOffer,
        Xdr::SetOptions => Ours::SetOptions,
        Xdr::ChangeTrust => Ours::ChangeTrust,
        Xdr::AllowTrust => Ours::AllowTrust,
        Xdr::AccountMerge => Ours::AccountMerge,
        Xdr::Inflation => Ours::Inflation,
        Xdr::ManageData => Ours::ManageData,
        Xdr::BumpSequence => Ours::BumpSequence,
        Xdr::ManageBuyOffer => Ours::ManageBuyOffer,
        Xdr::PathPaymentStrictSend => Ours::PathPaymentStrictSend,
        Xdr::CreateClaimableBalance => Ours::CreateClaimableBalance,
        Xdr::ClaimClaimableBalance => Ours::ClaimClaimableBalance,
        Xdr::BeginSponsoringFutureReserves => Ours::BeginSponsoringFutureReserves,
        Xdr::EndSponsoringFutureReserves => Ours::EndSponsoringFutureReserves,
        Xdr::RevokeSponsorship => Ours::RevokeSponsorship,
        Xdr::Clawback => Ours::Clawback,
        Xdr::ClawbackClaimableBalance => Ours::ClawbackClaimableBalance,
        Xdr::SetTrustLineFlags => Ours::SetTrustLineFlags,
        Xdr::LiquidityPoolDeposit => Ours::LiquidityPoolDeposit,
        Xdr::LiquidityPoolWithdraw => Ours::LiquidityPoolWithdraw,
        Xdr::InvokeHostFunction => Ours::InvokeHostFunction,
        Xdr::ExtendFootprintTtl => Ours::ExtendFootprintTtl,
        Xdr::RestoreFootprint => Ours::RestoreFootprint,
    }
}

#[test]
fn domain_operation_type_mirrors_stellar_xdr() {
    // `stellar_xdr::OperationType` discriminants are contiguous from 0; walk
    // until `TryFrom` rejects the next integer, which marks one past the last
    // variant.
    let mut count = 0i32;
    for disc in 0.. {
        let Ok(x) = Xdr::try_from(disc) else { break };
        assert_eq!(
            mirror(x) as i16,
            disc as i16,
            "discriminant drift: stellar_xdr {x:?} = {disc}, domain mirror = {}",
            mirror(x) as i16,
        );
        count += 1;
    }

    assert_eq!(
        count as usize,
        Ours::VARIANTS.len(),
        "variant count drift: stellar_xdr exposes {count}, domain::VARIANTS has {}",
        Ours::VARIANTS.len(),
    );
}
