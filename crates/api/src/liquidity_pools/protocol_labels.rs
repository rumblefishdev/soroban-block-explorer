//! Which protocol runs a soroban pool — the one place a deployment maps to a
//! protocol name (task 0374).
//!
//! A pool's `liquidity_pools.deployment_id` is the contract that registered
//! it: a router or a factory. An entry goes here only when the protocol's own
//! publications name that contract as theirs. Running the same code is not
//! enough: a second live router shares Aquarius's code byte for byte with
//! every admin role different (measured 2026-08-31), so code proves nothing
//! about who operates it.
//!
//! The list may be incomplete, never wrong: a deployment not listed here keeps
//! its pools indexed and serves `protocol: null` — no guessed name. Adding one
//! is a reviewed change to this file, with its evidence beside it.

use db_clickhouse::persist::ids;

use super::dto::PoolProtocol;

/// Deployment contract → protocol name, with the publication that names it.
///
/// - Aquarius router — the vendor's developer docs give it as "the contract ID
///   of the Aquarius AMM contract":
///   <https://docs.aqua.network/developers/code-examples/prerequisites-and-basics>
///   (checked 2026-09-28). 355 pools.
/// - Soroswap factory — the vendor's repository lists it as the mainnet
///   `factory`: `soroswap/core`, `public/mainnet.contracts.json` (checked
///   2026-09-28). 214 pools.
/// - Phoenix factory — the vendor's repository deploys and upgrades through it
///   on mainnet: `Phoenix-Protocol-Group/phoenix-contracts`,
///   `scripts/upgrade_mainnet.sh` (`FACTORY_ADDRESS`) and
///   `scripts/deploy_pool.sh` (checked 2026-09-28). 14 pools.
///
/// Pool counts from production, 2026-09-28: 583 of 775 soroban pools. The rest
/// come from routers and factories no protocol claims in its publications.
const DEPLOYMENTS: &[(&str, PoolProtocol)] = &[
    (
        "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK",
        PoolProtocol::Aquarius,
    ),
    (
        "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2",
        PoolProtocol::Soroswap,
    ),
    (
        "CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI",
        PoolProtocol::Phoenix,
    ),
];

/// The protocol that runs the pool registered by `deployment_id` (the
/// contract surrogate), or `None` when no protocol claims that deployment.
pub(super) fn protocol_of(deployment_id: i64) -> Option<PoolProtocol> {
    // ponytail: three hashes per call; a static map when the list grows.
    DEPLOYMENTS
        .iter()
        .find(|(contract, _)| ids::contract_id(contract) == deployment_id)
        .map(|(_, protocol)| *protocol)
}

#[cfg(test)]
mod tests;
