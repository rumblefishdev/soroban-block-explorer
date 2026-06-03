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

use crate::common::ch::{fetch_tx_list_aggregates, millis_to_utc};
use crate::common::cursor::{Direction, TsIdCursor, direction_sql};

use super::dto::SharesCursor;
use super::queries::{ParticipantRow, PoolRow, PoolTxRow};

/// 7-day freshness window expressed in ledgers (~17280 ledgers/day at the
/// ~5 s mainnet cadence). The PG path uses `snapshots.created_at >= NOW() - 7d`;
/// CH `liquidity_pool_snapshots` carries no `created_at`, so the window is
/// approximated by a `ledger_sequence` floor relative to chain head. Exact
/// wall-clock parity is a documented tolerance (freshness is a stale/fresh
/// heuristic, not an exact cutoff).
const FRESHNESS_WINDOW_LEDGERS: i64 = 7 * 17_280;

/// `true` if `s` is a plain decimal string (digits, at most one `.`, optional
/// leading `-`). Cursor `shares` is decoded from an opaque payload and inlined
/// into the keyset SQL (to dodge the clickhouse-rs None-into-tuple bind defect,
/// same as accounts/contracts); validating it first keeps that inline safe.
fn is_decimal_str(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && body.bytes().filter(|&b| b == b'.').count() <= 1
}

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

#[derive(Debug, Row, Deserialize)]
struct CountRow {
    n: u64,
}

/// `true` if a real (non-sentinel) pool with this id exists. Gates 404 vs
/// 200-empty on participants/transactions/chart. CH `liquidity_pools` has no
/// `created_at_ledger` sentinel column (dropped); a row's presence is the
/// existence signal. No FINAL needed — existence is unaffected by un-merged
/// duplicate versions.
pub async fn pool_exists(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<bool, clickhouse::error::Error> {
    let row = client
        .query("SELECT count() AS n FROM liquidity_pools WHERE pool_id = unhex(?)")
        .bind(pool_id_hex)
        .fetch_one::<CountRow>()
        .await?;
    Ok(row.n > 0)
}

#[derive(Debug, Row, Deserialize)]
struct ParticipantChRow {
    account: String,
    account_id_surrogate: i64,
    shares: String,
    share_percentage: Option<String>,
    first_deposit_ledger: i64,
    last_updated_ledger: i64,
}

/// `GET /v1/liquidity-pools/:id/participants` — active providers ordered by
/// `(shares DESC, account_id DESC)`. Mirrors the PG `fetch_participants`.
pub async fn fetch_participants(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    cursor: Option<&SharesCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<ParticipantRow>, clickhouse::error::Error> {
    let (op, order) = direction_sql(direction);

    // Keyset expanded out of the natural `(shares, account_id) <op> (?, ?)`
    // tuple form on purpose: a Decimal128 inside a CH tuple comparison is the
    // documented "Decimal-tuple-compare" trap. The scalar `shares <op>
    // toDecimal128(...)` is proven safe. The bounds are inlined (not bound) for
    // the same reason accounts/contracts inline theirs — a `None` bound into a
    // keyset returns an empty page on clickhouse-rs 0.15. `shares` is validated
    // as a decimal string before inlining; a tampered cursor degrades to "no
    // keyset" (first page) rather than injecting.
    let keyset = match cursor {
        Some(c) if is_decimal_str(&c.shares) => format!(
            "AND ((lpp.shares {op} toDecimal128('{s}', 7)) \
                  OR (lpp.shares = toDecimal128('{s}', 7) AND lpp.account_id {op} {a}))",
            op = op,
            s = c.shares,
            a = c.account_id,
        ),
        _ => String::new(),
    };

    // `snap.ts` = total_shares of the latest snapshot within the freshness
    // window (NULL → stale pool → share_percentage NULL). The scalar subquery
    // is scoped to the literal pool (not correlated). CROSS JOIN broadcasts the
    // single value to every position row (PG `LEFT JOIN latest_snap ON TRUE`).
    let sql = format!(
        "SELECT \
            acc.account_id                       AS account, \
            lpp.account_id                       AS account_id_surrogate, \
            toString(lpp.shares)                 AS shares, \
            if(snap.ts IS NULL OR snap.ts = toDecimal128(0, 7), NULL, \
               toString(lpp.shares * 100 / snap.ts)) AS share_percentage, \
            lpp.first_deposit_ledger             AS first_deposit_ledger, \
            lpp.last_updated_ledger              AS last_updated_ledger \
         FROM lp_positions lpp FINAL \
         JOIN accounts acc FINAL ON acc.id = lpp.account_id \
         CROSS JOIN ( \
            SELECT (SELECT total_shares FROM liquidity_pool_snapshots \
                     WHERE pool_id = unhex(?) \
                       AND ledger_sequence >= (SELECT max(sequence) FROM ledgers) - {fresh} \
                     ORDER BY ledger_sequence DESC LIMIT 1) AS ts \
         ) snap \
         WHERE lpp.pool_id = unhex(?) AND lpp.shares > 0 \
           {keyset} \
         ORDER BY lpp.shares {order}, lpp.account_id {order} \
         LIMIT ?",
        fresh = FRESHNESS_WINDOW_LEDGERS,
        keyset = keyset,
        order = order,
    );

    let rows = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(limit)
        .fetch_all::<ParticipantChRow>()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| ParticipantRow {
            account: r.account,
            account_id_surrogate: r.account_id_surrogate,
            shares: r.shares,
            share_percentage: r.share_percentage,
            first_deposit_ledger: r.first_deposit_ledger,
            last_updated_ledger: r.last_updated_ledger,
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct PoolTxChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    source_account: String,
    fee_charged: i64,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at_ms: i64,
}

/// `GET /v1/liquidity-pools/:id/transactions`. Mirrors the PG
/// `fetch_pool_transactions` (canonical SQL 20).
///
/// Two cost notes:
/// - **READ-COST PREREQUISITE — needs a projection before the flag flips.**
///   `operations_appearances` is ORDER BY `(ledger_sequence, …)`, so the
///   `pool_id` filter is NOT a key seek. Worse than the global tx-list
///   contract filter: LP-tagged ops are *sparse*, so the `ORDER BY
///   ledger_sequence DESC … LIMIT` driver does not fill early and scans the
///   whole table — box-measured **7.87B rows / 168 GB for a single page**,
///   ~80% of the hourly read_rows quota. This query is correct and
///   forward-compatible: a projection `ORDER BY (pool_id, ledger_sequence,
///   transaction_id)` on `operations_appearances` turns the `pool_id` filter
///   into a seek and CH auto-routes this exact query through it. Until that
///   projection is materialized, `API_DATASOURCE_LIQUIDITY_POOLS` MUST stay
///   off (the other four LP endpoints are pool_id-PK seeks and are cheap).
/// - The PG cursor keys on `(created_at, transaction_id)`. CH
///   `operations_appearances` has no `created_at`; since `ledgers.closed_at` is
///   a bijection with `ledger_sequence`, the cursor's `created_at` is mapped
///   back to its ledger via `fromUnixTimestamp64Milli` and the keyset runs on
///   `(ledger_sequence, transaction_id)`. `transaction_id` == `transactions.id`
///   (the FK), so the cursor `id` carries straight through.
pub async fn fetch_pool_transactions(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    direction: Direction,
) -> Result<Vec<PoolTxRow>, clickhouse::error::Error> {
    let (op, order) = direction_sql(direction);

    // Keyset: map the cursor's `created_at` (ms) back to its ledger sequence
    // (closed_at ↔ ledger_sequence bijection), then compare on
    // `(ledger_sequence, transaction_id)`. Bounds inlined (i64 ms + i64 id, no
    // injection) — same rationale as the other CH list paths.
    let keyset = match cursor {
        Some(c) => format!(
            "AND (oa.ledger_sequence {op} \
                  (SELECT sequence FROM ledgers WHERE closed_at = fromUnixTimestamp64Milli({ms}) LIMIT 1) \
               OR (oa.ledger_sequence = \
                   (SELECT sequence FROM ledgers WHERE closed_at = fromUnixTimestamp64Milli({ms}) LIMIT 1) \
                   AND oa.transaction_id {op} {id}))",
            op = op,
            ms = c.ts.timestamp_millis(),
            id = c.id,
        ),
        None => String::new(),
    };

    // limit*4 driver headroom (PG canonical pattern): a pool tx can touch the
    // pool via several ops; `LIMIT 1 BY` collapses to one key per transaction
    // before the page LIMIT.
    let lim_over = limit * 4;

    let sql = format!(
        "SELECT \
            t.id                                 AS id, \
            lower(hex(t.hash))                   AS hash, \
            t.ledger_sequence                    AS ledger_sequence, \
            src.account_id                       AS source_account, \
            t.fee_charged                        AS fee_charged, \
            t.successful                         AS successful, \
            t.operation_count                    AS operation_count, \
            t.has_soroban                        AS has_soroban, \
            toUnixTimestamp64Milli(l.closed_at)  AS created_at_ms \
         FROM ( \
            SELECT oa.ledger_sequence AS ls, oa.transaction_id AS tid \
            FROM operations_appearances oa \
            WHERE oa.pool_id = unhex(?) {keyset} \
            ORDER BY oa.ledger_sequence {order}, oa.transaction_id {order} \
            LIMIT 1 BY oa.ledger_sequence, oa.transaction_id \
            LIMIT {lim_over} \
         ) m \
         INNER JOIN transactions t ON t.id = m.tid AND t.ledger_sequence = m.ls \
         INNER JOIN accounts src ON src.id = t.source_id \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         ORDER BY t.ledger_sequence {order}, t.id {order} \
         LIMIT 1 BY t.id \
         LIMIT {limit}",
        keyset = keyset,
        order = order,
        lim_over = lim_over,
        limit = limit,
    );

    let page = client
        .query(&sql)
        .bind(pool_id_hex)
        .fetch_all::<PoolTxChRow>()
        .await?;

    if page.is_empty() {
        return Ok(Vec::new());
    }

    // operation_types via the shared non-correlated aggregate (ops-only, PK
    // seek on the page's tx keys).
    let keys: Vec<(i64, i64)> = page.iter().map(|r| (r.ledger_sequence, r.id)).collect();
    let aggregates = fetch_tx_list_aggregates(client, &keys).await?;

    Ok(page
        .into_iter()
        .map(|r| {
            let operation_types = aggregates
                .get(&r.id)
                .map(|a| a.operation_types.clone())
                .unwrap_or_default();
            PoolTxRow {
                id: r.id,
                hash: r.hash,
                ledger_sequence: r.ledger_sequence,
                source_account: r.source_account,
                fee_charged: r.fee_charged,
                successful: r.successful,
                operation_count: r.operation_count,
                has_soroban: r.has_soroban,
                operation_types,
                created_at: millis_to_utc(r.created_at_ms),
            }
        })
        .collect())
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
    fn decimal_str_validation() {
        assert!(is_decimal_str("0"));
        assert!(is_decimal_str("123.4567890"));
        assert!(is_decimal_str("-5.5"));
        assert!(!is_decimal_str(""));
        assert!(!is_decimal_str("1.2.3"));
        assert!(!is_decimal_str("1e9"));
        assert!(!is_decimal_str("'; DROP"));
        assert!(!is_decimal_str("abc"));
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
