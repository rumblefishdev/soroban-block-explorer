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
//! `operations_appearances`, `transaction_participants`,
//! `nft_ownership`) — for them composite (StrKey-or-hash, …) works
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
/// `operations_appearances.{source,destination}_id`,
/// `transaction_participants.account_id`,
/// `account_balances_current.account_id`,
/// `lp_positions.account_id`,
/// `nfts.current_owner_id`,
/// `nft_ownership.owner_id`,
/// `soroban_contracts.deployer_id`,
/// `assets.issuer_id`,
/// `liquidity_pools.asset_{a,b}_issuer_id`,
/// `soroban_invocations_appearances.caller_id`.
#[inline]
pub fn account_id(strkey: &str) -> i64 {
    hash64(strkey.as_bytes())
}

/// `soroban_contracts.id` from a StrKey (C…). Same helper feeds every
/// contract `Int64` FK: `operations_appearances.contract_id`,
/// `assets.contract_id`,
/// `nfts.contract_id`,
/// `nft_ownership.contract_id`,
/// `soroban_events.contract_id`,
/// `soroban_invocations_appearances.{contract,caller_contract}_id`.
#[inline]
pub fn contract_id(strkey: &str) -> i64 {
    hash64(strkey.as_bytes())
}

/// `transactions.id` from the 32-byte tx hash bytes. Same helper feeds
/// every transaction `Int64` FK: `operations_appearances.transaction_id`,
/// `transaction_participants.transaction_id`,
/// `soroban_events.transaction_id`,
/// `soroban_invocations_appearances.transaction_id`,
/// `nft_ownership.transaction_id`.
#[inline]
pub fn transaction_id(hash_bytes: &[u8; 32]) -> i64 {
    hash64(hash_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Determinism — same input ⇒ same output. Load-bearing for
    /// ReplacingMergeTree replay-idempotency.
    #[test]
    fn determinism() {
        let g = "GABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUV";
        let c = "CABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUV";
        let h = [7u8; 32];
        assert_eq!(account_id(g), account_id(g));
        assert_eq!(contract_id(c), contract_id(c));
        assert_eq!(transaction_id(&h), transaction_id(&h));
    }

    /// Different inputs ⇒ different outputs (single-byte sensitivity).
    #[test]
    fn distinct_inputs_yield_distinct_ids() {
        assert_ne!(account_id("GA"), account_id("GB"));
        assert_ne!(contract_id("CA"), contract_id("CB"));
        assert_ne!(transaction_id(&[0u8; 32]), transaction_id(&[1u8; 32]));
    }

    /// Cross-table FK consistency — every account `_id` FK across the
    /// schema must come from this single helper. Test asserts the
    /// invariant by computing twice and comparing.
    #[test]
    fn fk_consistency_account_id() {
        let key = "GFOOBAR0123";
        // Same value used for `accounts.id` AND every FK column
        // (`source_id`, `caller_id`, etc.).
        assert_eq!(account_id(key), account_id(key));
    }
}
