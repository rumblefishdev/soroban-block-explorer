use super::*;
use crate::liquidity_pools::queries::PoolRow;

fn base_row() -> PoolRow {
    PoolRow {
        pool_id_hex: "0".repeat(64),
        asset_a_type: 0,
        asset_a_type_name: Some("native".into()),
        asset_a_code: None,
        asset_a_issuer: None,
        asset_a_contract_id: None,
        asset_a_icon_url: None,
        asset_b_type: 1,
        asset_b_type_name: Some("credit_alphanum4".into()),
        asset_b_code: Some("USDC".into()),
        asset_b_issuer: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
        asset_b_contract_id: None,
        asset_b_icon_url: None,
        fee_bps: 30,
        fee_percent: "0.30".into(),
        created_at_ledger: 100,
        cursor_ledger: 100,
        participant_count: 0,
        latest_snapshot_ledger: None,
        reserve_a: None,
        reserve_b: None,
        total_shares: None,
        tvl: None,
        volume: None,
        fee_revenue: None,
        latest_snapshot_at: None,
    }
}

#[test]
fn native_leg_has_no_contract_id() {
    let item = map_pool_item(base_row());
    assert_eq!(item.asset_a.asset_type, 0, "asset_a is native");
    assert_eq!(item.asset_a.contract_id, None);
    assert_eq!(item.asset_b.asset_type, 1, "asset_b is classic credit");
}

#[test]
fn icon_url_propagates_per_leg() {
    // gap #5: each leg's icon_url threads from the row to the DTO leg,
    // independently. Native leg (no icon) stays None.
    let mut row = base_row();
    row.asset_b_icon_url = Some("https://cdn.example.test/icons/usdc.svg".into());
    let item = map_pool_item(row);
    assert_eq!(item.asset_a.icon_url, None, "native leg has no icon");
    assert_eq!(
        item.asset_b.icon_url.as_deref(),
        Some("https://cdn.example.test/icons/usdc.svg")
    );
}

#[test]
fn classic_credit_leg_surfaces_issuer_and_no_sac_mirror() {
    let item = map_pool_item(base_row());
    assert_eq!(item.asset_b.asset_code.as_deref(), Some("USDC"));
    assert!(item.asset_b.issuer.is_some());
    assert_eq!(
        item.asset_b.contract_id, None,
        "no SAC mirror in `assets` → contract_id stays None"
    );
}

#[test]
fn sac_mirror_contract_id_propagates_to_response() {
    let mut row = base_row();
    row.asset_b_contract_id =
        Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB".into());
    let item = map_pool_item(row);
    assert_eq!(
        item.asset_b.contract_id.as_deref(),
        Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB")
    );
}
