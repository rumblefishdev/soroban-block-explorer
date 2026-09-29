//! Soroban pool staging helpers: the registry rows of the three pool families,
//! the reserve and instance-state folds, and the key a pool leg token gets.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::HashMap;

use xdr_parser::types::ExtractedEvent;

use crate::persist::ids;
use crate::persist::rows::{LiquidityPoolRow, PoolInstanceStateRow, PoolStateChangeRow};

/// Collapse `pool_state_changes` to ONE row per (pool, plane, ledger) — the
/// cross-writer twin of `dedup_final_pool_snapshots` (lore-0356), via the
/// shared `keep_last_by_key` fold. This is the ONLY fold on this vector: a
/// pool's instance is rewritten several times in one busy ledger, and
/// emitting every image would leave the surviving row to a version-less
/// `ReplacingMergeTree` — the hazard backfills.md rule 4 names.
///
/// `plane_id` is IN the key (three-lens review, 2026-09-01): a forged plane
/// entry naming a real pool would otherwise EVICT the pool's genuine row at
/// this fold (and at the table's RMT key) — the read-side declared-plane
/// filter would then hide the forgery but serve a stale ledger's reserves as
/// current. With the plane in the key a forged row lands in its own key
/// space, dies at the read filter, and stays visible to the divergence
/// monitor (see the `pool_state_changes` DDL comment). Genuine collisions
/// still fold: both arms stamp the pool's own declared plane.
///
/// Last-wins in staging order: the instance arm is the more specific source
/// for a concentrated pool and runs second. Folding — not a version column —
/// is what makes the stored row a deterministic function of the ledger, so a
/// re-parse still wins simply by landing last (rule 4). The `ledger_sequence`
/// component is belt-and-braces for a future batching caller
/// (`xdr_parser::fold`).
pub(super) fn fold_pool_state_changes(rows: Vec<PoolStateChangeRow>) -> Vec<PoolStateChangeRow> {
    xdr_parser::fold::keep_last_by_key(rows, |r| (r.pool_id, r.plane_id, r.ledger_sequence))
}

/// Collapse instance-state rows to ONE per (pool, ledger) — the last image
/// in apply order. Load-bearing, not theoretical: 29 of 259 real
/// (pool, ledger) keys in the raw corpus carry more than one instance image
/// (up to 5 in one ledger — every pool action rewrites the instance), and
/// the table's RMT version (`derived_at_ledger`) TIES within a ledger, so an
/// unfolded insert would leave the surviving image to an arbitrary merge —
/// the 0463 defect class. Every image carries the FULL instance storage, so
/// keeping only the last loses nothing.
pub(super) fn fold_pool_instance_state(
    rows: Vec<PoolInstanceStateRow>,
) -> Vec<PoolInstanceStateRow> {
    xdr_parser::fold::keep_last_by_key(rows, |r| (r.pool_id, r.derived_at_ledger))
}

/// Raw decimal reserve strings → `i128`, all-or-nothing: one unparseable
/// element refuses the whole vector, because a partial reserve set is a
/// snapshot lying about its own arity. Used by the router-family instance
/// arm.
pub(super) fn parse_reserves(raw: &[String]) -> Option<Vec<i128>> {
    raw.iter().map(|r| r.parse::<i128>().ok()).collect()
}

/// The two-leg tuple flavour of [`parse_reserves`], shared by the
/// pair-factory and config-factory arms — same all-or-nothing rule.
pub(super) fn parse_reserve_pair(a: &str, b: &str) -> Option<Vec<i128>> {
    Some(vec![a.parse::<i128>().ok()?, b.parse::<i128>().ok()?])
}

/// Raw LP-supply string → `i128`. Absent means the write did not touch the
/// key (a structural 0, never a fallback); PRESENT-but-unparseable is
/// `Err` so the caller refuses the row loudly. Shared by the pair-factory
/// (TotalSupply) and config-factory (TotalShares) arms.
pub(super) fn parse_supply(raw: Option<&str>) -> Result<i128, ()> {
    match raw {
        None => Ok(0),
        Some(raw) => raw.parse::<i128>().map_err(|_| ()),
    }
}

/// The `assets.id` surrogate for a token named by its CONTRACT address —
/// a soroban pool leg, or a contract-held balance.
///
/// Such a token is one of two things, and only one of them may keep its own
/// contract surrogate. A genuine Soroban token IS its contract as far as asset
/// identity goes (`ids::asset_id`'s type-3 arm returns `contract_id`). A SAC is
/// NOT: ADR 0051 retired `asset_type = 2`, so a SAC has no `assets` row of its
/// own and a leg keyed on its surrogate points at nothing — the same orphaning
/// `build_balance_rows` exists to prevent for contract-held balances, and the
/// reason 1,084 of 1,175 soroban legs resolved to no asset at all (task 0374,
/// measured on production 2026-09-08).
///
/// `sac_classic` is the SAME map the balance path uses (seeded with this
/// ledger's own SAC carriers before either caller runs), so both paths key the
/// same asset identically. A token ABSENT from the map is a Soroban-native
/// token, and it goes through `ids::asset_id` rather than returning the
/// contract surrogate directly — the two are equal only because that is what
/// the type-3 arm does, and spelling it as an ASSET id is what the defect
/// above was missing.
///
/// Both callers key the same asset identically because they are the same
/// function. They were two copies, which had already drifted cosmetically (one
/// spelled the family `3`, the other named the enum).
#[inline]
pub(super) fn contract_token_asset_id(token: &str, sac_classic: &HashMap<i64, i64>) -> i64 {
    let contract = ids::contract_id(token);
    sac_classic
        .get(&contract)
        .copied()
        .unwrap_or_else(|| ids::asset_id(domain::AssetFamily::Soroban as i16, "", 0, contract))
}

/// Whether this ledger's events register any soroban pool, in any of the three
/// families — i.e. whether [`contract_token_asset_id`] will be asked for a leg.
pub fn registers_soroban_pools(events: &[(String, Vec<ExtractedEvent>)]) -> bool {
    !xdr_parser::pool_router::detect_pool_registrations(events).is_empty()
        || !xdr_parser::pool_pair_factory::detect_pair_registrations(events).is_empty()
        || !xdr_parser::pool_config_factory::detect_config_pool_registrations(events).is_empty()
}

/// Registry row for one corroborated `new_pair` registration (task 0518).
///
/// `pool_type_raw` stays EMPTY: the vendor emits no type — Soroswap is one
/// fixed constant-product mode — and an invented label would be our
/// interpretation, not a verbatim value (decision 64). The fee is the
/// vendor's compiled-in constant: 3/1000 on every swap ("Constant product
/// AMM with a .3% swap fee", `soroswap/core` pair source, fetched
/// 2026-09-02) = 30 bps. Legs are the pair's leg TOKENS in vendor order
/// (token_0, token_1); the share token is NOT a registry column — the pair
/// is its own LP token and the relation lives in `pool_instance_state`.
pub(super) fn factory_pair_registry_row(
    reg: &xdr_parser::pool_pair_factory::PairRegistration,
    ledger_sequence: i64,
    sac_classic: &HashMap<i64, i64>,
) -> Result<LiquidityPoolRow, &'static str> {
    let pool_id =
        ids::contract_payload(&reg.event.pair).ok_or("pair address is not a valid C… strkey")?;
    Ok(LiquidityPoolRow {
        pool_id,
        asset_a_type: 0,
        asset_a_code: String::new(),
        asset_a_issuer_id: 0,
        asset_b_type: 0,
        asset_b_code: String::new(),
        asset_b_issuer_id: 0,
        fee_bps: 30,
        last_updated_ledger: ledger_sequence,
        pool_kind: 1,
        legs: vec![
            contract_token_asset_id(&reg.event.token_0, sac_classic),
            contract_token_asset_id(&reg.event.token_1, sac_classic),
        ],
        deployment_id: ids::contract_id(&reg.factory),
        pool_type_raw: String::new(),
    })
}

/// Registry row for one corroborated config-factory registration. The event
/// names only the pool; every registry fact comes from the pool's own
/// `CONFIG` (the same map that corroborated the registration). The share
/// token is NOT a registry column — the relation lives in
/// `pool_instance_state`, same as both sibling families.
///
/// `pool_type_raw` stores the vendor's `PairType` discriminant verbatim
/// ("0" = XYK today; a stable pool would carry its own value) — the same
/// un-normalised-on-purpose rule as the router family's sym.
pub(super) fn config_pool_registry_row(
    reg: &xdr_parser::pool_config_factory::ConfigPoolRegistration,
    config: &xdr_parser::pool_config_factory::PoolConfig,
    ledger_sequence: i64,
    sac_classic: &HashMap<i64, i64>,
) -> Result<LiquidityPoolRow, &'static str> {
    let pool_id =
        ids::contract_payload(&reg.pool).ok_or("pool address is not a valid C… strkey")?;
    // The chain carries the fee as i64; an out-of-i32-range value is a new
    // vocabulary nobody has seen — refuse it loudly rather than record a
    // plausible truncation.
    let fee_bps =
        i32::try_from(config.total_fee_bps).map_err(|_| "total_fee_bps out of i32 range")?;
    Ok(LiquidityPoolRow {
        pool_id,
        asset_a_type: 0,
        asset_a_code: String::new(),
        asset_a_issuer_id: 0,
        asset_b_type: 0,
        asset_b_code: String::new(),
        asset_b_issuer_id: 0,
        fee_bps,
        last_updated_ledger: ledger_sequence,
        pool_kind: 1,
        legs: vec![
            contract_token_asset_id(&config.token_a, sac_classic),
            contract_token_asset_id(&config.token_b, sac_classic),
        ],
        deployment_id: ids::contract_id(&reg.factory),
        pool_type_raw: config.pool_type.to_string(),
    })
}

/// Registry row for one decoded `add_pool` registration.
///
/// `Err` names what was wrong. A bad pool address must never fabricate a
/// 32-byte `pool_id`, and an unparseable fee must never become a plausible
/// zero (Karol, 2026-08-28: error, not warn-and-default) — either way the
/// registration is refused loudly and lands in the missing-pool alarm.
///
/// The share-token relation lives ONLY in `pool_instance_state` (side table;
/// a registry column for it was dead-on-arrival and removed). No venue label
/// is stored anywhere: labels resolve from `deployment_id` at read time. The salt and raw
/// init_args are NOT materialised — the add_pool event itself sits complete
/// in soroban_events; extract on demand, never copy.
pub(super) fn pool_registry_row(
    reg: &xdr_parser::pool_router::AddPoolEvent,
    router_strkey: &str,
    ledger_sequence: i64,
    sac_classic: &HashMap<i64, i64>,
) -> Result<LiquidityPoolRow, &'static str> {
    let pool_id =
        ids::contract_payload(&reg.pool).ok_or("pool address is not a valid C… strkey")?;
    // Position 0 is a u32 fee in EVERY shape measured on mainnet (497/497,
    // pinned by the corpus test). A shape where it is missing or unparseable
    // is a new vocabulary nobody has seen — refuse it loudly rather than
    // record a plausible fee of 0.
    let fee_bps = reg
        .init_args
        .first()
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or("init_args[0] is not a parseable fee")?;
    Ok(LiquidityPoolRow {
        pool_id,
        asset_a_type: 0,
        asset_a_code: String::new(),
        asset_a_issuer_id: 0,
        asset_b_type: 0,
        asset_b_code: String::new(),
        asset_b_issuer_id: 0,
        fee_bps,
        last_updated_ledger: ledger_sequence,
        pool_kind: 1,
        legs: reg
            .tokens
            .iter()
            .map(|t| contract_token_asset_id(t, sac_classic))
            .collect(),
        deployment_id: ids::contract_id(router_strkey),
        pool_type_raw: reg.pool_type.clone(),
    })
}
