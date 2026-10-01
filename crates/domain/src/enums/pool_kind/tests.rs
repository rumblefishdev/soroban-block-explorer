use super::*;

#[test]
fn round_trip() {
    for v in PoolKind::VARIANTS {
        assert_eq!(PoolKind::try_from(*v as i16).unwrap(), *v);
        assert_eq!(v.as_str().parse::<PoolKind>().unwrap(), *v);
    }
}

/// The two labels must not collide with the asset vocabulary: `soroban`
/// appears in both, and a renderer that mixes them is the 0496 incident
/// again. Pinned so a rename here has to think about it.
#[test]
fn soroban_means_the_pool_here_not_the_asset() {
    assert_eq!(PoolKind::Soroban.as_str(), "soroban");
    assert_eq!(domain_asset_family_soroban(), "soroban");
}

fn domain_asset_family_soroban() -> &'static str {
    super::super::AssetFamily::Soroban.as_str()
}
