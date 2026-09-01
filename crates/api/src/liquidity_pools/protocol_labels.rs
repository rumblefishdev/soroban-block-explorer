//! Verified-operator protocol labels (task 0374 T1) — the ONLY place a
//! deployment id maps to a protocol name.
//!
//! # How an entry earns its place
//!
//! An address goes in when the OPERATOR's identity is verified against the
//! vendor's own publications (docs / repo / site naming the deployment).
//! Code identity is NOT enough: a second live router shares Aquarius's WASM
//! byte-for-byte with all seven admin roles disjoint (measured), so "same
//! code" proves nothing about who operates it.
//!
//! # Failure direction
//!
//! The dictionary is allowed to be INCOMPLETE, never wrong: an unlisted
//! deployment's pools stay fully indexed and render with NO protocol chip
//! (`protocol: null`), instead of a guessed label. Adding a protocol is a
//! reviewed code change here, not an UPDATE — so every label in prod has a
//! commit explaining its evidence.

use std::collections::HashMap;

use crate::common::ch::resolve_contracts;

/// Router deployments whose OPERATOR identity is verified against vendor
/// documentation (task 0374 T1). Resolved at read time from
/// `liquidity_pools.deployment_id` → `soroban_contracts.contract_id` → this
/// list, so a new pool is labelled the moment it registers and a label fix
/// is a code change, not an UPDATE.
///
/// Deliberately NOT "every deployment of Aquarius's WASM": a second live
/// router shares the code byte-for-byte with all seven admin roles disjoint
/// (measured), so code identity does not establish operator identity. Its
/// pools stay indexed and unlabelled.
///
/// # Evidence per entry
///
/// - `aquarius` — the vendor's own developer docs publish exactly this
///   address as "The contract ID of the Aquarius AMM contract"
///   (`router_contract_id`, mainnet):
///   <https://docs.aqua.network/developers/code-examples/prerequisites-and-basics>
///   (re-checked live 2026-08-31; also captured 2026-03-26 in lore research
///   0003 `sources/aquarius-docs-prerequisites.md`, and 2026-03-27 in 0008
///   `sources/aqua-docs-soroban-functions.md` — that second page has since
///   moved and 404s today, which is why captures exist).
const ROUTER_PROTOCOL_LABELS: &[(&str, &str)] = &[(
    "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK",
    "aquarius",
)];

/// `deployment_id` surrogates → protocol labels, via the contract dimension.
/// Ids absent from [`ROUTER_PROTOCOL_LABELS`] simply don't appear in the map
/// (unlabelled ≠ error).
pub(crate) async fn resolve_protocol_labels(
    client: &clickhouse::Client,
    deployment_ids: Vec<i64>,
) -> Result<HashMap<i64, &'static str>, clickhouse::error::Error> {
    let ids: Vec<i64> = deployment_ids.into_iter().filter(|&id| id != 0).collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let strkeys = resolve_contracts(client, ids).await?;
    Ok(strkeys
        .into_iter()
        .filter_map(|(id, strkey)| {
            ROUTER_PROTOCOL_LABELS
                .iter()
                .find(|(router, _)| *router == strkey)
                .map(|(_, label)| (id, *label))
        })
        .collect())
}
