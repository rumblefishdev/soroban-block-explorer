use super::*;

/// Reduce with an empty SAC registry — these tests use native/credit deltas
/// only, which don't need it. The SAC-registry path is covered separately.
fn reduce(deltas: &[LedgerDelta]) -> Vec<NetSettled> {
    ledger_deltas_net_settled(deltas, &HashMap::new())
}

const ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";
const G_A: &str = "GBLVLKGRDU66WLWY4XRORJXCC4LDZ347AQTUYBEPBABIZTVITW2OAGIP";
const G_B: &str = "GADKLS7RS3OC2MXGEZXQA46JNF3FBVSTHTWLDPRF7TWI6GXVP4OUE3ZR";

fn native(account: &str, delta: i128) -> LedgerDelta {
    LedgerDelta {
        account: account.to_string(),
        asset: LedgerAsset::Native,
        delta,
    }
}
fn credit(account: &str, delta: i128) -> LedgerDelta {
    LedgerDelta {
        account: account.to_string(),
        asset: LedgerAsset::Credit {
            code: "USDC".to_string(),
            issuer: ISSUER.to_string(),
        },
        delta,
    }
}

#[test]
fn native_payment_nets_to_amount() {
    // A -100, B +100: net native 100.
    let r = reduce(&[native(G_A, -100), native(G_B, 100)]);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].asset_id, r[0].amount),
        (ids::NATIVE_ASSET_ID, Some(100))
    );
}

#[test]
fn credit_delta_resolves_to_its_surrogate() {
    let want = ids::credit_asset_id("USDC", ISSUER);
    let r = reduce(&[credit(G_A, -150)]);
    assert_eq!((r[0].asset_id, r[0].amount), (want, Some(150)));
}

#[test]
fn one_sided_burn_delta_is_counted() {
    // A single negative delta (e.g. clawback / payment-to-issuer): max(Σ+,Σ−)
    // keeps it non-zero.
    let r = reduce(&[native(G_A, -250)]);
    assert_eq!(r[0].amount, Some(250));
}

#[test]
fn swap_splits_into_two_asset_rows() {
    // A sends 300 native, receives 250 USDC; B is the counterparty.
    let usdc = ids::credit_asset_id("USDC", ISSUER);
    let r = reduce(&[
        native(G_A, -300),
        native(G_B, 300),
        credit(G_A, 250),
        credit(G_B, -250),
    ]);
    assert_eq!(r.len(), 2);
    assert_eq!(
        r.iter()
            .find(|n| n.asset_id == ids::NATIVE_ASSET_ID)
            .unwrap()
            .amount,
        Some(300)
    );
    assert_eq!(
        r.iter().find(|n| n.asset_id == usdc).unwrap().amount,
        Some(250)
    );
}

#[test]
fn contract_and_account_legs_of_a_sac_transfer_net_as_one_asset() {
    // A contract sends 100 USDC to a G-account. The contract leg is a
    // ContractData SAC balance (SacWrapped), the account leg is a trustline
    // (Credit). Both MUST resolve to the SAME asset_id via the registry, or the
    // single transfer double-counts as two assets. (Verified in code that
    // sac_classic maps to exactly `credit_asset_id`; this pins it.)
    let usdc = ids::credit_asset_id("USDC", ISSUER);
    let sac_strkey = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    let sac_classic: HashMap<i64, i64> =
        [(ids::contract_id(sac_strkey), usdc)].into_iter().collect();

    let contract_leg = LedgerDelta {
        account: "CONTRACT_HOLDER".to_string(),
        asset: LedgerAsset::SacWrapped(sac_strkey.to_string()),
        delta: -100,
    };
    let r = ledger_deltas_net_settled(&[contract_leg, credit(G_B, 100)], &sac_classic);
    assert_eq!(r.len(), 1, "must net as ONE asset, not double-count: {r:?}");
    assert_eq!((r[0].asset_id, r[0].amount), (usdc, Some(100)));
}

#[test]
fn bespoke_contract_token_resolves_to_its_own_surrogate() {
    let token = "CBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N";
    let d = LedgerDelta {
        account: "HOLDER".to_string(),
        asset: LedgerAsset::Bespoke(token.to_string()),
        delta: -50,
    };
    let r = ledger_deltas_net_settled(&[d], &HashMap::new());
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].asset_id, r[0].amount),
        (ids::contract_id(token), Some(50))
    );
}
