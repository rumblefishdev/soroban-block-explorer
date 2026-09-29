//! Deterministic surrogate-ID derivation for the three high-cardinality
//! FK hubs in the CH schema: `accounts`, `soroban_contracts`,
//! `transactions`.
//!
//! ## Why surrogate IDs on these three (and not elsewhere)
//!
//! These three tables are each referenced by 6–8 downstream tables.
//! At Stellar mainnet scale they accumulate tens of millions of unique
//! values. Empirical measurement on the 10k-ledger smoke
//! (62016000–62025999) showed a natural-key-everywhere variant added
//! ~500 MB on disk vs a hash-i64 baseline — projected ~550 GB at
//! 11M-ledger full scale. Int64 FK columns also enable significantly
//! cheaper JOIN / GROUP BY: CH compares integers in a single CPU op
//! vs variable-length string memcmp + per-byte hashing.
//!
//! Other tables stay on natural / composite primary keys (`assets`,
//! `nfts`, `liquidity_pools`, `lp_positions`, `liquidity_pool_snapshots`,
//! `transaction_operations`, `transaction_participants`,
//! `nft_ownership_changes`) — for them composite (StrKey-or-hash, …) works
//! cheaply without a hash layer.
//!
//! ## Determinism is load-bearing
//!
//! Same natural key → same `id`, always. Three reasons:
//!
//! 1. **Replay-idempotency for `ReplacingMergeTree`.** Dedup keys on
//!    `ORDER BY` (which on these tables references `id` either
//!    directly via FK or indirectly via the ORDER BY's natural-key
//!    spelling). Replays must produce identical `id` for identical
//!    natural keys or the merger never collapses duplicates from a
//!    re-run.
//! 2. **Parallel-writer safety.** K runners on disjoint partition
//!    ranges all compute the same `id` for the same StrKey without
//!    coordination — no shared counter, no Keeper, no race.
//! 3. **Cross-table FK consistency.** Every `source_id` / `account_id`
//!    / `caller_id` / etc. across the schema uses the same helper
//!    here, so JOIN `ON accounts.id = transactions.source_id` is a
//!    trivial integer equality match.
//!
//! ## Algorithm
//!
//! `cityhash-rs::cityhash_102_128` (CityHash v1.0.2 128-bit) — already
//! present in the dep graph as a transitive of `clickhouse`'s
//! `lz4`/`zstd` features. The lower 64 bits become the row's `Int64`.
//!
//! ### Deliberate divergence from CH SQL `cityHash64()`
//!
//! ClickHouse's built-in `cityHash64()` function is the **64-bit**
//! variant of CityHash v1.0.2 — a different algorithm from the
//! lower-half of the 128-bit variant. The writer's surrogate IDs are
//! therefore **not** bit-equivalent to running
//! `SELECT cityHash64(natural_key)` from CH SQL. Future CH-side
//! `JOIN ... ON cityHash64(natural_key) = id` queries need a UDF
//! wrapping this helper or recompute via the `db-clickhouse` crate.
//!
//! Acceptable trade — adding `cityhash-102-rs` (which would be
//! CH-bit-compatible) requires network access for `cargo update`
//! at lock time, and the v1.0.2 128-bit variant we already have
//! in-tree gives equivalent collision and distribution properties.

use cityhash_rs::cityhash_102_128;

#[inline]
fn hash64(bytes: &[u8]) -> i64 {
    // Lower 64 bits of u128. Two's-complement reinterpretation is
    // intentional — the bits are what get serialized over RowBinary;
    // signed reading is never relied on.
    cityhash_102_128(bytes) as u64 as i64
}

/// `accounts.id` from a StrKey (G…). Same helper feeds every account
/// `Int64` FK in the schema: `transactions.source_id`,
/// `transaction_operations.{source,destination}_id`,
/// `transaction_participants.account_id`,
/// `account_balances_current.account_id`,
/// `lp_positions.account_id`,
/// `nfts.current_owner_id`,
/// `nft_ownership_changes.owner_id`,
/// `soroban_contracts.deployer_id`,
/// `assets.issuer_id`,
/// `liquidity_pools.asset_{a,b}_issuer_id`,
/// `contract_activity.caller_id`.
#[inline]
pub fn account_id(strkey: &str) -> i64 {
    hash64(strkey.as_bytes())
}

/// `soroban_contracts.id` from a StrKey (C…). Same helper feeds every
/// contract `Int64` FK: `transaction_operations.contract_id`,
/// `assets.contract_id`,
/// `nfts.contract_id`,
/// `nft_ownership_changes.contract_id`,
/// `soroban_events.contract_id`,
/// `contract_activity.{contract,caller_contract}_id`.
#[inline]
pub fn contract_id(strkey: &str) -> i64 {
    hash64(strkey.as_bytes())
}

/// 32-byte payload of a C… contract StrKey, or `None` when it is not one.
/// Feeds `FixedString(32)` pool ids for soroban rows (`liquidity_pools`,
/// `pool_state_changes`, `pool_instance_state`) — a payload taken from
/// anything other than a valid contract StrKey would fabricate a pool id.
pub fn contract_payload(strkey: &str) -> Option<[u8; 32]> {
    match stellar_strkey::Strkey::from_string(strkey) {
        Ok(stellar_strkey::Strkey::Contract(c)) => Some(c.0),
        _ => None,
    }
}

/// `balances.holder_id` from ANY `ScAddress` StrKey — a G-account or a C-contract
/// (a balance holder can be either; task 0331). Same `cityhash64` as
/// `account_id`/`contract_id` (one shared surrogate space; resolve back to a StrKey
/// via `accounts` (G) / `soroban_contracts` (C)).
#[inline]
pub fn address_id(strkey: &str) -> i64 {
    hash64(strkey.as_bytes())
}

/// `assets.id` / `balances.asset_id` surrogate (task 0331). Takes the SURROGATE
/// FKs (`issuer_id`, `contract_id` — both already `cityhash64` of their StrKey)
/// that `AssetRow` carries. By `assets.asset_type` (project enum: 0 native,
/// 1 classic_credit, 2 sac, 3 soroban):
/// - native: `cityhash64("native")`
/// - classic_credit: `cityhash64("CODE:<issuer_id>")`
/// - soroban (type-3): **the `contract_id` surrogate itself** — a token's
///   `balances.asset_id` equals its own contract surrogate.
///
/// The `_` arm also covers the **RETIRED type-2 (SAC)**: post-ADR-0051 / task 0339
/// a SAC is a FACET of its classic/native asset, NOT a separate asset. Its
/// contract-held balances are re-keyed onto the WRAPPED classic/native id in
/// [`crate::persist::stage::build_balance_rows`], so NO type-2 `asset_id` is ever
/// persisted. `_ => contract_id` is kept only so the fn stays total on an
/// unexpected legacy type-2 input; `AssetFamily::TryFrom` already rejects 2.
///
/// The native XLM asset surrogate — `asset_id(0, "", 0, 0)` = `hash64("native")`,
/// a stable FIRST-CLASS key (task 0359 / ADR 0051), never the empty-string
/// sentinel. Named so call sites read as "the native asset" instead of the magic
/// empty 4-tuple; the golden test pins `NATIVE_ASSET_ID == asset_id(0, "", 0, 0)`.
pub const NATIVE_ASSET_ID: i64 = -6_959_166_271_784_855_184;

/// Deterministic (replay-idempotent), no central counter.
#[inline]
pub fn asset_id(asset_type: i16, asset_code: &str, issuer_id: i64, contract_id: i64) -> i64 {
    match asset_type {
        0 => hash64(b"native"),
        1 => hash64(format!("{asset_code}:{issuer_id}").as_bytes()),
        // soroban (type-3): the contract surrogate IS the asset id. The retired
        // type-2 (SAC) also lands here, but its balances are re-keyed to the classic
        // id (ADR 0051), so a type-2 result is never stored.
        _ => contract_id,
    }
}

/// The classic credit-asset surrogate for `asset_code` issued by G-StrKey
/// `issuer` — i.e. `asset_id(1, asset_code, account_id(issuer), 0)`. The single
/// canonical resolution shared by the `operation_asset_appearances` presence
/// rows and the value-moved rows (task 0393) and by the classic + Soroban value
/// paths, so their keys always match. Call this instead of re-spelling the
/// 4-arg formula, so the surrogate scheme lives in exactly one place.
#[inline]
pub fn credit_asset_id(asset_code: &str, issuer: &str) -> i64 {
    asset_id(1, asset_code, account_id(issuer), 0)
}

/// The surrogate for one leg of a liquidity pool, from the `liquidity_pools`
/// columns (`asset_a_type` / `asset_a_code` / `asset_a_issuer_id`).
///
/// **`asset_type` there is NOT the project enum [`asset_id`] takes.** It is the
/// raw XDR asset type, where `1` is `credit_alphanum4` and `2` is
/// `credit_alphanum12` — both ordinary classic credit assets. In the project
/// enum `2` means the retired SAC facet, so feeding a pool leg straight into
/// [`asset_id`] sends every 12-character code into the `_ => contract_id` arm
/// and yields `0`, an id nothing is ever stored under (task 0489: that silently
/// blanked one leg of every trade on 59% of pools).
///
/// A pool leg is classic by construction — native or credit, never a contract —
/// so both credit widths collapse onto the one credit surrogate the writer uses
/// (`stage.rs::claim_atom_asset_id` → [`credit_asset_id`]).
///
/// The three XDR types are matched EXPLICITLY rather than via an `else`, so
/// that a fourth one has to be thought about instead of silently inheriting the
/// credit formula. Soroban-AMM indexing (issue #405) would bring exactly that:
/// a leg holding a Soroban token, whose real surrogate is its contract id. An
/// `else` would hand back a well-formed hash matching no row — the same silent
/// blank this function exists to fix, and harder to spot than the `0` was.
#[inline]
pub fn pool_leg_asset_id(asset_type: i16, asset_code: &str, issuer_id: i64) -> i64 {
    match asset_type {
        // XDR `ASSET_TYPE_NATIVE`.
        0 => NATIVE_ASSET_ID,
        // XDR `CREDIT_ALPHANUM4` / `CREDIT_ALPHANUM12` — both classic credit,
        // one surrogate, keyed on (code, issuer) with no width in the formula.
        1 | 2 => asset_id(1, asset_code, issuer_id, 0),
        // Unreachable from a classic AMM pool, whose XDR `Asset` admits only the
        // three above. Loud rather than fatal: a wrong id blanks a cell, while a
        // panic here would take down a read path over a display value.
        other => {
            debug_assert!(
                false,
                "pool leg asset_type {other} is not an XDR asset type — \
                 pool_leg_asset_id needs a real surrogate for it, not the credit formula",
            );
            tracing::warn!(
                asset_type = other,
                "unexpected pool leg asset_type; falling back to the classic-credit \
                 surrogate, which will not match any pool_operation_amounts row",
            );
            asset_id(1, asset_code, issuer_id, 0)
        }
    }
}

#[cfg(test)]
mod tests;
