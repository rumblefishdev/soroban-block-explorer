//! ClickHouse queries for the liquidity-pool endpoints (task 0243).
//!
//! Mirrors the PG path (`queries.rs`) — same `PoolRow` / `PoolTxRow` /
//! `ParticipantRow` / `ChartDataPoint` shapes, so the handlers reuse
//! `map_pool_item` / cursor builders unchanged after the fetch.
//!
//! CH-specific translation choices (see task 0243 handoff note):
//! - **Decimal128(7)** columns are read via `toString(...)` in SQL → wire
//!   decimal strings, sidestepping the clickhouse-rs Decimal decode gotcha.
//! - **`pool_id`** is a `FixedString(32)`; the wire/hex form is the 64-char
//!   lowercase hex. SQL compares with `pool_id = unhex(?)` and reads back
//!   `lower(hex(pool_id))`.
//! - **`created_at_ledger`** does NOT exist on CH `liquidity_pools` (dropped,
//!   see schema header) — derived as `min(ledger_sequence)` over the pool's
//!   snapshots, falling back to `last_updated_ledger` for a pool that somehow
//!   has no snapshot yet.
//! - **snapshot `created_at`** does NOT exist on CH `liquidity_pool_snapshots`
//!   (only `ledger_sequence`) — the latest-snapshot timestamp is derived from
//!   the joined `ledgers.closed_at`.
//! - The freshness window (PG: `snapshots.created_at >= NOW() - 7d`) is NOT
//!   applied on the detail/list latest-snapshot pick yet — detail takes the
//!   single latest snapshot regardless of age (matches the "latest known
//!   state" intent); a staleness cutoff is a follow-up if parity needs it.

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::millis_to_utc;

use super::queries::PoolRow;

/// `asset_type` SMALLINT → label, matching the PG `asset_type_name()` SQL
/// function (migration `20260422000000_enum_label_functions`) — the XDR
/// `AssetType`, which is what LP legs carry. NOT `token_asset_type_name`
/// (native/classic_credit/sac/soroban); the `PoolAssetLeg` doc-comment quotes
/// that sibling function and is misleading for pool legs. Box-confirmed: a
/// 9-char code (`WGUARDIAN`) is `asset_type = 2` = credit_alphanum12, not sac.
/// Out-of-range → `None` (PG `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native".to_string()),
        1 => Some("credit_alphanum4".to_string()),
        2 => Some("credit_alphanum12".to_string()),
        3 => Some("pool_share".to_string()),
        _ => None,
    }
}

/// `fee_bps / 100` as a decimal string (e.g. 30 → "0.3", 25 → "0.25",
/// 100 → "1"). Computed in Rust to avoid CH integer-division / decimal-scale
/// quirks; trailing zeros are trimmed.
///
/// NOTE: PG emits `(fee_bps::numeric / 100)::text`; exact trailing-zero
/// parity is a documented box-smoke check (cosmetic field, FE re-renders).
fn fee_percent_str(fee_bps: i32) -> String {
    let whole = fee_bps / 100;
    let frac = (fee_bps % 100).abs();
    if frac == 0 {
        whole.to_string()
    } else if frac % 10 == 0 {
        format!("{whole}.{}", frac / 10)
    } else {
        format!("{whole}.{frac:02}")
    }
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolDetailChRow {
    pool_id_hex: String,
    asset_a_type: i16,
    asset_a_code: Option<String>,
    asset_a_issuer: Option<String>,
    asset_a_contract_id: Option<String>,
    asset_b_type: i16,
    asset_b_code: Option<String>,
    asset_b_issuer: Option<String>,
    asset_b_contract_id: Option<String>,
    fee_bps: i32,
    created_at_ledger: i64,
    participant_count: i64,
    latest_snapshot_ledger: Option<i64>,
    reserve_a: Option<String>,
    reserve_b: Option<String>,
    total_shares: Option<String>,
    tvl: Option<String>,
    volume: Option<String>,
    fee_revenue: Option<String>,
    latest_snapshot_at_ms: Option<i64>,
}

/// `GET /v1/liquidity-pools/:id` — single-pool detail. Mirrors the PG
/// `fetch_pool_by_id` projection.
pub async fn fetch_pool_by_id(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolRow>, clickhouse::error::Error> {
    // `unhex(?)` appears 4×: outer WHERE, the latest-snapshot subquery, the
    // participant-count subquery, and the created_at-ledger subquery. The
    // subqueries are scoped to the literal pool id (NOT correlated to `lp`),
    // both because detail is single-pool and because CH dislikes correlated
    // subqueries. Each `?` consumes one positional bind.
    let row = client
        .query(
            "SELECT \
                lower(hex(lp.pool_id))               AS pool_id_hex, \
                lp.asset_a_type                      AS asset_a_type, \
                nullIf(lp.asset_a_code, '')          AS asset_a_code, \
                nullIf(iss_a.account_id, '')         AS asset_a_issuer, \
                nullIf(sac_a.contract_id, '')        AS asset_a_contract_id, \
                lp.asset_b_type                      AS asset_b_type, \
                nullIf(lp.asset_b_code, '')          AS asset_b_code, \
                nullIf(iss_b.account_id, '')         AS asset_b_issuer, \
                nullIf(sac_b.contract_id, '')        AS asset_b_contract_id, \
                lp.fee_bps                           AS fee_bps, \
                ifNull( \
                    (SELECT min(ledger_sequence) FROM liquidity_pool_snapshots \
                      WHERE pool_id = unhex(?)), \
                    lp.last_updated_ledger)          AS created_at_ledger, \
                (SELECT count() FROM lp_positions FINAL \
                  WHERE pool_id = unhex(?) AND shares > 0) AS participant_count, \
                s.ledger_sequence                    AS latest_snapshot_ledger, \
                toString(s.reserve_a)                AS reserve_a, \
                toString(s.reserve_b)                AS reserve_b, \
                toString(s.total_shares)             AS total_shares, \
                toString(s.tvl)                      AS tvl, \
                toString(s.volume)                   AS volume, \
                toString(s.fee_revenue)              AS fee_revenue, \
                toUnixTimestamp64Milli(l.closed_at)  AS latest_snapshot_at_ms \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN accounts iss_a FINAL ON iss_a.id = lp.asset_a_issuer_id \
             LEFT JOIN accounts iss_b FINAL ON iss_b.id = lp.asset_b_issuer_id \
             LEFT JOIN assets sac_a_row \
                    ON sac_a_row.asset_code = lp.asset_a_code \
                   AND sac_a_row.issuer_id  = lp.asset_a_issuer_id \
                   AND sac_a_row.asset_type = 2 \
             LEFT JOIN soroban_contracts sac_a ON sac_a.id = sac_a_row.contract_id \
             LEFT JOIN assets sac_b_row \
                    ON sac_b_row.asset_code = lp.asset_b_code \
                   AND sac_b_row.issuer_id  = lp.asset_b_issuer_id \
                   AND sac_b_row.asset_type = 2 \
             LEFT JOIN soroban_contracts sac_b ON sac_b.id = sac_b_row.contract_id \
             LEFT JOIN ( \
                 SELECT ledger_sequence, reserve_a, reserve_b, total_shares, tvl, volume, fee_revenue \
                 FROM liquidity_pool_snapshots FINAL \
                 WHERE pool_id = unhex(?) \
                 ORDER BY ledger_sequence DESC \
                 LIMIT 1 \
             ) s ON 1 = 1 \
             LEFT JOIN ledgers l ON l.sequence = s.ledger_sequence \
             WHERE lp.pool_id = unhex(?) \
             LIMIT 1",
        )
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .fetch_optional::<PoolDetailChRow>()
        .await?;

    Ok(row.map(|r| PoolRow {
        pool_id_hex: r.pool_id_hex,
        asset_a_type: r.asset_a_type,
        asset_a_type_name: asset_type_name(r.asset_a_type),
        asset_a_code: r.asset_a_code,
        asset_a_issuer: r.asset_a_issuer,
        asset_a_contract_id: r.asset_a_contract_id,
        asset_b_type: r.asset_b_type,
        asset_b_type_name: asset_type_name(r.asset_b_type),
        asset_b_code: r.asset_b_code,
        asset_b_issuer: r.asset_b_issuer,
        asset_b_contract_id: r.asset_b_contract_id,
        fee_bps: r.fee_bps,
        fee_percent: fee_percent_str(r.fee_bps),
        created_at_ledger: r.created_at_ledger,
        participant_count: r.participant_count,
        latest_snapshot_ledger: r.latest_snapshot_ledger,
        reserve_a: r.reserve_a,
        reserve_b: r.reserve_b,
        total_shares: r.total_shares,
        tvl: r.tvl,
        volume: r.volume,
        fee_revenue: r.fee_revenue,
        latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_percent_formats() {
        assert_eq!(fee_percent_str(30), "0.3");
        assert_eq!(fee_percent_str(25), "0.25");
        assert_eq!(fee_percent_str(100), "1");
        assert_eq!(fee_percent_str(0), "0");
        assert_eq!(fee_percent_str(5), "0.05");
    }

    #[test]
    fn asset_type_names() {
        assert_eq!(asset_type_name(0).as_deref(), Some("native"));
        assert_eq!(asset_type_name(1).as_deref(), Some("credit_alphanum4"));
        assert_eq!(asset_type_name(2).as_deref(), Some("credit_alphanum12"));
        assert_eq!(asset_type_name(3).as_deref(), Some("pool_share"));
        assert_eq!(asset_type_name(9), None);
    }
}
