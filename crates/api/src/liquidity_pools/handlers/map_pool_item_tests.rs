use super::*;
use crate::liquidity_pools::queries::{PoolLegRow, PoolRow};

fn native_leg() -> PoolLegRow {
    PoolLegRow {
        family: domain::AssetFamily::Native as i16,
        asset_code: None,
        issuer: None,
        contract_id: None,
        symbol: None,
        icon_url: None,
    }
}

fn usdc_leg() -> PoolLegRow {
    PoolLegRow {
        family: domain::AssetFamily::ClassicCredit as i16,
        asset_code: Some("USDC".into()),
        issuer: Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
        contract_id: None,
        symbol: None,
        icon_url: None,
    }
}

fn base_row() -> PoolRow {
    PoolRow {
        pool_id_hex: "0".repeat(64),
        pool_kind: domain::PoolKind::Classic as i16,
        legs: vec![native_leg(), usdc_leg()],
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
fn legs_carry_the_family_vocabulary_not_the_xdr_one() {
    let item = map_pool_item(base_row());
    assert_eq!(item.legs.len(), 2);
    assert_eq!(item.legs[0].asset_type_name.as_deref(), Some("native"));
    // `classic_credit`, NOT `credit_alphanum4`: one vocabulary across the
    // API, the one `/v1/assets` already speaks and the frontend maps.
    assert_eq!(
        item.legs[1].asset_type_name.as_deref(),
        Some("classic_credit")
    );
}

/// A soroban token with no classic code is named by its symbol, which the
/// leg must carry — without it the pool pages fell back to the address while
/// global search, reading the same identity, showed the symbol.
#[test]
fn a_soroban_leg_carries_its_symbol() {
    let mut row = base_row();
    row.legs[1] = PoolLegRow {
        family: domain::AssetFamily::Soroban as i16,
        asset_code: None,
        issuer: None,
        contract_id: Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB".into()),
        symbol: Some("USDx".into()),
        icon_url: None,
    };
    let item = map_pool_item(row);
    assert_eq!(item.legs[1].symbol.as_deref(), Some("USDx"));
    // Classic legs name no contract of their own.
    assert_eq!(item.legs[0].contract_id, None);
}

#[test]
fn icon_url_propagates_per_leg() {
    let mut row = base_row();
    row.legs[1].icon_url = Some("https://cdn.example.test/icons/usdc.svg".into());
    let item = map_pool_item(row);
    assert_eq!(item.legs[0].icon_url, None, "native leg has no icon");
    assert_eq!(
        item.legs[1].icon_url.as_deref(),
        Some("https://cdn.example.test/icons/usdc.svg")
    );
}

/// Three legs are not a hypothetical — stable pools carry them on mainnet,
/// and the pair shape this replaced could not express one.
#[test]
fn a_three_leg_pool_renders_all_three() {
    let mut row = base_row();
    row.legs.push(PoolLegRow {
        family: domain::AssetFamily::Soroban as i16,
        asset_code: None,
        issuer: None,
        contract_id: Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB".into()),
        symbol: None,
        icon_url: None,
    });
    let item = map_pool_item(row);
    assert_eq!(item.legs.len(), 3);
    assert_eq!(item.legs[2].asset_type_name.as_deref(), Some("soroban"));
    // A soroban token IS its contract, so that is the link target.
    assert_eq!(
        item.legs[2].contract_id.as_deref(),
        Some("CAQCFVLOBK5GIULPNZRGSXFPMIDUTBDDKCEHQNCZGYNK5JEN6IY5RZQB")
    );
}

/// The id encoding follows the KIND. Both forms are well-formed for the
/// same 32 bytes, so rendering the wrong one is silent, not an error.
#[test]
fn the_pool_id_renders_by_kind() {
    let classic = map_pool_item(base_row());
    assert!(classic.pool_id.starts_with('L'), "{}", classic.pool_id);
    assert_eq!(classic.pool_kind.as_deref(), Some("classic"));

    let mut row = base_row();
    row.pool_kind = domain::PoolKind::Soroban as i16;
    let soroban = map_pool_item(row);
    assert!(soroban.pool_id.starts_with('C'), "{}", soroban.pool_id);
    assert_eq!(soroban.pool_kind.as_deref(), Some("soroban"));
}
