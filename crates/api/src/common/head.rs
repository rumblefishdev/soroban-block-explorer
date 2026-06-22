//! Cheap chain-head lookup: the latest `ledgers.sequence`.
//!
//! This is the shared "version" source for freshness primitives that must
//! decide *whether* anything changed before paying for a heavy query:
//!
//! - **Version-keyed caching** (task 0291): the in-process network-stats
//!   cache keys on the head, so a new ledger becomes visible on the *first*
//!   request after it is written instead of after a TTL window expires.
//! - **Conditional GET / `ETag`** (task 0292): `ETag = latest_ledger_sequence`
//!   short-circuits to `304 Not Modified` when the head is unchanged.
//!
//! Both layers MUST take the head from here so there is a single, cheap
//! head source rather than two independent (and divergent) reads.
//!
//! ## Why it is cheap (and why the two engines use different SQL)
//!
//! Both forms read a single row off the ordering key, not the row body — and
//! this matters: with version-keying the head is read on **every** request
//! (not just per cache miss), so its cost is multiplied by the full request
//! rate. A form that scanned even a moderate slice would risk the CH
//! `read_rows` quota that this codebase has already tripped (`QUOTA_EXCEEDED`,
//! see `common/ch.rs`). Each form is therefore the provably-minimal idiom for
//! its engine:
//!
//! - **PG** — `SELECT max(sequence)`. `sequence` is the `BIGINT PRIMARY KEY`
//!   (migration `0002_identity_and_ledgers`), and Postgres rewrites
//!   `max()`/`min()` over an indexed column into an `Index Only Scan Backward`
//!   with `LIMIT 1`: one index entry (plus at most one heap page for MVCC
//!   visibility), no heap scan. Microseconds.
//! - **CH** — `SELECT sequence ORDER BY sequence DESC LIMIT 1`, **not**
//!   `max(sequence)`. `ledgers` is `ORDER BY (sequence)` (schema `init.sql`),
//!   so this is a read-in-order over the sorting key: a **single granule**,
//!   guaranteed regardless of whether the server optimises `max()` over the
//!   sorting key into an index-only read (which is version/setting dependent).
//!   This mirrors the documented one-granule "latest ledger" idiom in
//!   `network/queries_ch.rs`.
//!
//! Both are orders of magnitude cheaper than the network-stats statement they
//! gate.
//!
//! ## Empty-table sentinel
//!
//! A cold-bootstrap cluster with no ingested ledgers yields `0`: both queries
//! return zero rows (PG `max()` returns one `NULL` row), mapped to `0`.
//! Stellar genesis is ledger 1, so `0` is an unambiguous "nothing ingested
//! yet" head — consistent with the zero-valued empty-cluster response in
//! `network/queries{,_ch}.rs`.

use sqlx::PgPool;

/// Latest `ledgers.sequence` from Postgres (B-tree max over the PK).
/// Returns `0` for an empty `ledgers` table (see module docs).
pub async fn latest_sequence_pg(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let head: Option<i64> = sqlx::query_scalar("SELECT max(sequence) FROM ledgers")
        .fetch_one(pool)
        .await?;
    Ok(head.unwrap_or(0))
}

/// Latest `ledgers.sequence` from ClickHouse: a one-granule read-in-order over
/// the `ORDER BY (sequence)` key (see module docs for why this, not `max()`).
/// Returns `0` for an empty `ledgers` table (zero rows → `None`). The bare
/// scalar decode mirrors the indexer's head probe
/// (`crates/indexer/src/handler/mod.rs`).
pub async fn latest_sequence_ch(
    client: &clickhouse::Client,
) -> Result<i64, clickhouse::error::Error> {
    let head = client
        .query("SELECT sequence FROM ledgers ORDER BY sequence DESC LIMIT 1")
        .fetch_optional::<i64>()
        .await?;
    Ok(head.unwrap_or(0))
}
