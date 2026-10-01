use super::protocol_of;

/// `soroban_contracts.id` of each listed deployment on production
/// (2026-09-28) — the value `liquidity_pools.deployment_id` carries. Pins
/// that the address hashes to the stored surrogate, so a typo in an address
/// fails here rather than silently leaving its pools unlabelled.
#[test]
fn listed_deployments_match_their_production_ids() {
    assert_eq!(protocol_of(5_490_683_486_378_605_019), Some("Aquarius"));
    assert_eq!(protocol_of(-8_559_314_389_216_369_730), Some("Soroswap"));
    assert_eq!(protocol_of(2_808_977_402_438_572_953), Some("Phoenix"));
}

#[test]
fn an_unlisted_or_absent_deployment_has_no_protocol() {
    // A classic pool's deployment is 0; any other id is a deployment nobody
    // claims.
    assert_eq!(protocol_of(0), None);
    assert_eq!(protocol_of(42), None);
}
