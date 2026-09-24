//! `GET /v1/liquidity-pools/:id` — the single-pool read.

use clickhouse::Row;
use serde::Deserialize;
use std::collections::BTreeSet;

use crate::common::asset_identity::resolve_identities_and_icons;
use crate::common::ch::millis_to_utc;
use crate::common::strkey::decode_pool_kind;

use super::leg_reserves::{Reserves, state_reserves_sql};
use super::total_shares::{instance_shares_sql, total_shares_of, zero_shares_is_measured};
use super::{PoolRow, fee_percent_str, leg_rows};

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolDetailChRow {
    pool_id_hex: String,
    pool_kind: i16,
    legs: Vec<i64>,
    fee_bps: i32,
    created_at_ledger: i64,
    participant_count: i64,
    latest_snapshot_ledger: Option<i64>,
    reserve_a: Option<String>,
    reserve_b: Option<String>,
    total_shares: Option<String>,
    latest_snapshot_at_ms: Option<i64>,
    /// Verbatim family marker; read only by [`zero_shares_is_measured`].
    pool_type_raw: String,
    /// A soroban pool's latest reserves, raw and in leg order; empty for a
    /// classic pool, which has none there.
    state_reserves: Vec<String>,
    /// A soroban pool's raw instance-state shares, scaled in Rust (a `u128`).
    instance_shares: Option<String>,
    instance_shares_decimals: Option<u32>,
}

/// `GET /v1/liquidity-pools/:id` — single-pool detail. Mirrors the PG
/// `fetch_pool_by_id` projection. `tvl`/`volume`/`fee_revenue` are NOT read
/// here — the snapshot columns were never populated (pre-0199 design); the
/// handler fills them from [`fetch_pool_usd_analytics`] (compute-at-read).
pub async fn fetch_pool_by_id(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolRow>, clickhouse::error::Error> {
    // `unhex(?)` appears 8×: the created_at-ledger and participant-count
    // subqueries, the latest-snapshot and ledger seeks, the soroban reserves
    // (twice) and shares joins, and the outer WHERE. All scoped to the literal pool id (NOT correlated to `lp`) since
    // detail is single-pool and CH dislikes correlated subqueries. Each `?`
    // consumes one positional bind; all are the same value, so order is moot.
    //
    // **Leg identity is resolved in Rust, not joined here.** This used to carry
    // three pair-keyed CTEs — `legs` (the pool's four pair columns), `iss` (a
    // bounded issuer seek) and `sac` (the SAC mirror + icon, two hops through
    // `asset_sac` and `soroban_contracts`) — plus four joins onto them. All of
    // it keyed on `(asset_code, issuer_id)`, which only a classic row has.
    // `legs` stores `assets.id` surrogates for both pool kinds, so the same
    // work is one batched call to the shared resolver, which already carries
    // the shapes those joins had to get right — including the issuer seek that
    // must never be an `accounts FINAL` join (a 14M-row hash, `Code 241`).
    //
    // **Latest snapshot subquery — NO `FINAL`** (0356 / PR #318). The indexer now
    // writes exactly one deterministic row per `(pool_id, ledger_sequence)`, so
    // `FINAL` is redundant for dedup; dropping it turns the read into a bounded
    // reverse-PK seek (`ORDER BY ledger_sequence DESC LIMIT 1`) instead of a
    // whole-table merge. It stays a whole-row `LIMIT 1` (not per-column
    // `argMax`), so `reserve_a`/`reserve_b` can never tear across a stale
    // before/after pair in the pre-cleanup window. `created_at_ledger` already
    // reads without `FINAL` (`min(ledger_sequence)` is dup-invariant).
    //
    // **`ledgers` is SEEKED, never joined whole.** `LEFT JOIN ledgers l ON
    // l.sequence = s.ledger_sequence` hash-built the entire 26M-row table to
    // resolve ONE `closed_at` — 27.2M read_rows / 1.82 GiB / ~724 ms of CH per
    // request under load (96% of this endpoint's cost, measured 2026-07-17 at
    // 50M/mo). Restricting the right side to the single sequence the snapshot
    // subquery points at makes it a PK point read.
    //
    // The `s` join is an EQUI-join on `pool_id` (not `ON 1 = 1`): a constant ON
    // condition is only supported by `join_algorithm = 'hash'`, so the old form
    // 500'd (Code 48) the moment the server profile carried anything else.
    let sql = format!(
        "SELECT \
                lower(hex(lp.pool_id))               AS pool_id_hex, \
                toInt16(lp.pool_kind)                AS pool_kind, \
                lp.legs                              AS legs, \
                lp.fee_bps                           AS fee_bps, \
                ifNull( \
                    (SELECT min(ledger_sequence) FROM liquidity_pool_snapshots \
                      WHERE pool_id = unhex(?)), \
                    lp.last_updated_ledger)          AS created_at_ledger, \
                toInt64(ifNull( \
                    (SELECT count() FROM lp_positions FINAL \
                      WHERE pool_id = unhex(?) AND shares > 0), 0)) AS participant_count, \
                s.ledger_sequence                    AS latest_snapshot_ledger, \
                toString(s.reserve_a)                AS reserve_a, \
                toString(s.reserve_b)                AS reserve_b, \
                toString(s.total_shares)             AS total_shares, \
                nullIf(toUnixTimestamp64Milli(l.closed_at), 0) AS latest_snapshot_at_ms, \
                lp.pool_type_raw                     AS pool_type_raw, \
                sr.reserves                          AS state_reserves, \
                inst.shares_raw                      AS instance_shares, \
                inst.shares_decimals                 AS instance_shares_decimals \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN ( \
                 SELECT pool_id, \
                        toNullable(ledger_sequence) AS ledger_sequence, \
                        toNullable(reserve_a)       AS reserve_a, \
                        toNullable(reserve_b)       AS reserve_b, \
                        toNullable(total_shares)    AS total_shares \
                 FROM liquidity_pool_snapshots \
                 WHERE pool_id = unhex(?) \
                 ORDER BY ledger_sequence DESC \
                 LIMIT 1 \
             ) s ON s.pool_id = lp.pool_id \
             LEFT JOIN ( \
                 SELECT sequence, closed_at FROM ledgers \
                 WHERE sequence = (SELECT max(ledger_sequence) FROM liquidity_pool_snapshots \
                                    WHERE pool_id = unhex(?)) \
             ) l ON l.sequence = s.ledger_sequence \
             LEFT JOIN ({reserves}) sr ON sr.pool_id = lp.pool_id \
             LEFT JOIN ({shares}) inst ON inst.pool_id = lp.pool_id \
             WHERE lp.pool_id = unhex(?) \
             LIMIT 1",
        reserves = state_reserves_sql("unhex(?)", "0"),
        shares = instance_shares_sql("unhex(?)"),
    );
    let mut query = client.query(&sql);
    for _ in 0..8 {
        query = query.bind(pool_id_hex);
    }
    let row = query.fetch_optional::<PoolDetailChRow>().await?;

    let Some(r) = row else { return Ok(None) };
    let leg_ids: BTreeSet<i64> = r.legs.iter().copied().collect();
    let (identities, icons) = resolve_identities_and_icons(client, &leg_ids).await?;

    Ok(Some(PoolRow {
        pool_kind: decode_pool_kind(&r.pool_id_hex, r.pool_kind),
        pool_id_hex: r.pool_id_hex,
        // A soroban pool's reserves come from its state changes; a classic
        // pool's legs are its two snapshot columns, in order.
        legs: leg_rows(
            &r.legs,
            &identities,
            &icons,
            if r.state_reserves.is_empty() {
                Reserves::Pair(r.reserve_a.as_deref(), r.reserve_b.as_deref())
            } else {
                Reserves::Raw(&r.state_reserves)
            },
        ),
        fee_bps: r.fee_bps,
        fee_percent: fee_percent_str(r.fee_bps),
        created_at_ledger: r.created_at_ledger,
        // Detail does not paginate; the field is set for struct completeness.
        cursor_ledger: r.created_at_ledger,
        participant_count: r.participant_count,
        latest_snapshot_ledger: r.latest_snapshot_ledger,
        total_shares: total_shares_of(
            r.total_shares,
            r.instance_shares.as_deref(),
            r.instance_shares_decimals,
            zero_shares_is_measured(&r.pool_type_raw, &r.state_reserves),
        ),
        // Filled by the handler from `fetch_pool_usd_analytics` (0199
        // compute-at-read); the snapshot columns are not read.
        tvl: None,
        volume: None,
        fee_revenue: None,
        latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
    }))
}
