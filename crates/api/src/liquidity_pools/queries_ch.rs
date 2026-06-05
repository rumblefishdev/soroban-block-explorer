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

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{fetch_tx_list_aggregates, millis_to_utc};
use crate::common::cursor::{Direction, TsIdCursor, direction_sql};

use super::dto::{ChartDataPoint, SharesCursor};
use super::queries::{ParticipantRow, PoolRow, PoolTxRow, ResolvedPoolListParams};

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

/// `true` if `s` is a 64-char lowercase-hex `pool_id` (the wire form, decoded
/// from the opaque list cursor). Guards the inlined keyset bound: a `pool_id`
/// from a tampered cursor that is not clean hex degrades to "no keyset" (first
/// page) rather than reaching the SQL string. Same rationale as
/// [`is_decimal_str`] for the participants cursor.
fn is_hex_pool_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
                   AND lp.asset_a_code != '' \
             LEFT JOIN soroban_contracts sac_a ON sac_a.id = sac_a_row.contract_id \
             LEFT JOIN assets sac_b_row \
                    ON sac_b_row.asset_code = lp.asset_b_code \
                   AND sac_b_row.issuer_id  = lp.asset_b_issuer_id \
                   AND sac_b_row.asset_type = 2 \
                   AND lp.asset_b_code != '' \
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
        // Detail does not paginate; the field is set for struct completeness.
        cursor_ledger: r.created_at_ledger,
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

#[derive(Debug, Row, Deserialize)]
struct ChartChRow {
    bucket_ms: i64,
    tvl: Option<String>,
    volume: Option<String>,
    fee_revenue: Option<String>,
    samples_in_bucket: u64,
}

/// `GET /v1/liquidity-pools/:id/chart` — time-bucketed TVL / volume / fee
/// series. Mirrors the PG `fetch_pool_chart` (canonical SQL 21).
///
/// CH translation choices:
/// - **Bucket truncation** maps the `1h | 1d | 1w` allowlist to
///   `toStartOfHour` / `toStartOfDay` / `toMonday`. `toMonday` is the
///   Monday-start week, matching PG's ISO `date_trunc('week', …)`; the
///   epoch-aligned `toStartOfInterval(…, INTERVAL 604800 SECOND)` from the
///   reference SQL is Sunday-aligned and would drift a day off PG.
/// - **No `created_at` on CH snapshots** — the window is filtered on the
///   joined `ledgers.closed_at` (bijection with `ledger_sequence`), so the
///   `from`/`to` API contract (RFC3339 timestamps) is preserved unchanged
///   rather than switched to the ledger-bound form the reference SQL used.
/// - **`pool_id = unhex(?)`** is a leading-PK seek on
///   `liquidity_pool_snapshots` (`ORDER BY (pool_id, ledger_sequence)`), so
///   the scan is bounded to this pool's snapshots — box-measured 14.5 M rows
///   / 237 MB for the hottest pool (1.84 M snapshots) over a 90-day 1d window.
/// - **TVL** is a state quantity → `argMax(tvl, ledger_sequence)` (latest in
///   bucket); **volume** / **fee_revenue** are flow quantities → `sum()`. CH
///   `sum()` over an all-NULL bucket yields NULL (box-confirmed), matching PG
///   `SUM` → no 0-vs-NULL drift.
pub async fn fetch_pool_chart(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ChartDataPoint>, clickhouse::error::Error> {
    // Defensive second gate (the handler validates against the allowlist
    // first) — fail loud on allowlist drift rather than emit a wrong bucket.
    assert!(
        matches!(interval, "1h" | "1d" | "1w"),
        "fetch_pool_chart called with non-allowlisted interval `{interval}` — \
         handler validation drift; expected 1h | 1d | 1w"
    );
    let bucket_fn = match interval {
        "1h" => "toStartOfHour",
        "1d" => "toStartOfDay",
        "1w" => "toMonday",
        _ => unreachable!("interval validated against the 1h|1d|1w allowlist above"),
    };

    // `bucket_ms`: each truncated bucket is coerced to a UTC `DateTime64(3)`
    // then to epoch millis, so `millis_to_utc` round-trips it on the Rust side
    // (matches the `DateTime<Utc>` shape PG returns from `date_trunc`).
    let sql = format!(
        "SELECT \
            toUnixTimestamp64Milli(toDateTime64({bucket_fn}(l.closed_at), 3, 'UTC')) AS bucket_ms, \
            toString(argMax(lps.tvl, lps.ledger_sequence)) AS tvl, \
            toString(sum(lps.volume))                      AS volume, \
            toString(sum(lps.fee_revenue))                 AS fee_revenue, \
            count()                                        AS samples_in_bucket \
         FROM liquidity_pool_snapshots lps FINAL \
         JOIN ledgers l ON l.sequence = lps.ledger_sequence \
         WHERE lps.pool_id = unhex(?) \
           AND l.closed_at >= fromUnixTimestamp64Milli(?) \
           AND l.closed_at <  fromUnixTimestamp64Milli(?) \
         GROUP BY bucket_ms \
         ORDER BY bucket_ms ASC",
        bucket_fn = bucket_fn,
    );

    let rows = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(from.timestamp_millis())
        .bind(to.timestamp_millis())
        .fetch_all::<ChartChRow>()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| ChartDataPoint {
            bucket: millis_to_utc(r.bucket_ms),
            tvl: r.tvl,
            volume: r.volume,
            fee_revenue: r.fee_revenue,
            samples_in_bucket: r.samples_in_bucket as i64,
        })
        .collect())
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolListChRow {
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
    /// `last_updated_ledger` — the list sort/cursor key (see fn doc).
    cursor_ledger: i64,
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

/// `GET /v1/liquidity-pools` — paginated pool list. Mirrors the PG
/// `fetch_pool_list` projection, with two CH-specific structural choices
/// driven by the box-measured read cost (`liquidity_pool_snapshots` = 268 M
/// rows):
///
/// - **Order key = `last_updated_ledger` (NOT `created_at_ledger`).** PG keys
///   on `created_at_ledger` (pool creation). CH `liquidity_pools` dropped that
///   column (PR #175); its only in-window proxy — `min(snapshot
///   ledger_sequence)` — is clamped to the frozen backfill floor (≈ L50.4M)
///   for every pre-window pool, so it is useless as an order key (mass ties)
///   *and* would force a full 268 M-row snapshot GROUP BY just to derive it.
///   `last_updated_ledger` is a native non-NULL column → the list pages the
///   small `liquidity_pools` table (51 k rows) FIRST, then seeks snapshots /
///   positions for only the page's ≤ limit+1 pools (`pool_id` is their leading
///   PK). Box-measured ≈ 55 M rows/page vs ≈ 268 M for the full-scan shape.
///   The wire `created_at_ledger` field still reports the min-snapshot proxy
///   (parity with detail); only the *ordering* differs, and the FE does not
///   consume the list yet, so there is no live ordering regression.
/// - **`min_tvl` filter** is the one case that cannot page-first (TVL is
///   snapshot-derived, so it changes page membership): a `tvl_pools` pre-filter
///   CTE does the full-scan `argMax(tvl)` GROUP BY (268 M rows, box-measured
///   333 MB, no OOM) and the page CTE intersects it. Opt-in + rare; currently
///   returns 0 pools because `tvl` is unpopulated (task 0199).
///
/// Read-cost note for the eventual flag flip: the per-page ≈ 55 M is dominated
/// by the `accounts` id→strkey issuer resolution (14 M, non-PK reverse lookup)
/// and the `ledgers` closed_at join; both are bounded and the list is
/// user-initiated (not polled). The `operations_appearances` projection that
/// blocks the transactions endpoint does NOT block the list.
pub async fn fetch_pool_list(
    client: &clickhouse::Client,
    params: &ResolvedPoolListParams,
    direction: Direction,
) -> Result<Vec<PoolRow>, clickhouse::error::Error> {
    let (op, order) = direction_sql(direction);

    // Keyset on `(last_updated_ledger, pool_id)`, expanded to scalar
    // comparisons. The cursor's `created_at_ledger` slot carries
    // `last_updated_ledger` on the CH path (opaque, ADR 0008). Bounds inlined:
    // `cursor_ledger` is i64 (no injection); `pool_id_hex` is validated hex.
    // A tampered/non-hex cursor degrades to "no keyset" (first page).
    let keyset = match params.cursor.as_ref() {
        Some(c) if is_hex_pool_id(&c.pool_id_hex) => format!(
            "AND ((lp.last_updated_ledger {op} {cl}) \
                  OR (lp.last_updated_ledger = {cl} \
                      AND lower(hex(lp.pool_id)) {op} '{ph}'))",
            op = op,
            cl = c.created_at_ledger,
            ph = c.pool_id_hex,
        ),
        _ => String::new(),
    };

    // `min_tvl` pre-filter — full-scan `argMax(tvl)` GROUP BY (see fn doc).
    // The handler already validated the decimal shape; `is_decimal_str` re-guards
    // the inline. Invalid → filter skipped (handler guarantees it never is).
    let (tvl_cte, tvl_pred) = match params.min_tvl.as_deref() {
        Some(m) if is_decimal_str(m) => (
            format!(
                "tvl_pools AS ( \
                    SELECT pool_id, argMax(tvl, ledger_sequence) AS latest_tvl \
                    FROM liquidity_pool_snapshots FINAL \
                    GROUP BY pool_id \
                    HAVING latest_tvl >= toDecimal128('{m}', 7) \
                 ),"
            ),
            " AND lp.pool_id IN (SELECT pool_id FROM tvl_pools)".to_string(),
        ),
        _ => (String::new(), String::new()),
    };

    // Asset filters are bound (untrusted free-text codes / handler-validated
    // issuer StrKeys — clickhouse-rs escapes them). Each `?` appears in the
    // `page` CTE WHERE in this exact push order. Issuer StrKey → surrogate id
    // resolves via an `accounts` PK seek (`ORDER BY (account_id)`), cheap.
    let mut binds: Vec<String> = Vec::new();
    let mut filters = String::new();
    if let Some(code) = params.asset_a_code.as_ref() {
        filters.push_str(" AND lp.asset_a_code = ?");
        binds.push(code.clone());
    }
    if let Some(iss) = params.asset_a_issuer.as_ref() {
        filters.push_str(
            " AND lp.asset_a_issuer_id = \
              (SELECT id FROM accounts FINAL WHERE account_id = ? LIMIT 1)",
        );
        binds.push(iss.clone());
    }
    if let Some(code) = params.asset_b_code.as_ref() {
        filters.push_str(" AND lp.asset_b_code = ?");
        binds.push(code.clone());
    }
    if let Some(iss) = params.asset_b_issuer.as_ref() {
        filters.push_str(
            " AND lp.asset_b_issuer_id = \
              (SELECT id FROM accounts FINAL WHERE account_id = ? LIMIT 1)",
        );
        binds.push(iss.clone());
    }
    if let Some(code) = params.asset_code.as_ref() {
        filters.push_str(" AND (upper(lp.asset_a_code) = ? OR upper(lp.asset_b_code) = ?)");
        binds.push(code.clone());
        binds.push(code.clone());
    }

    // `created_at_ledger` / latest-snapshot fields wrap the aggregates in
    // `toNullable(...)` so a no-snapshot pool yields NULL (not the 0/'' default)
    // on the LEFT JOIN miss — `join_use_nulls` is rejected for the read-only
    // CH user, so this is the readonly-safe NULL path. (Every current pool has
    // ≥ 1 snapshot, so this is defensive.) `nullIf(...)` does the same for the
    // empty-string-sentinel string columns. Native legs (asset_code = '') are
    // excluded from the SAC join by the `lp.asset_*_code != ''` guard so they
    // surface a NULL `contract_id`, matching PG (NULL code → no SAC match).
    let sql = format!(
        "WITH \
         {tvl_cte} \
         page AS ( \
             SELECT lp.pool_id AS pool_id, lp.asset_a_type AS asset_a_type, \
                    lp.asset_a_code AS asset_a_code, lp.asset_a_issuer_id AS asset_a_issuer_id, \
                    lp.asset_b_type AS asset_b_type, lp.asset_b_code AS asset_b_code, \
                    lp.asset_b_issuer_id AS asset_b_issuer_id, lp.fee_bps AS fee_bps, \
                    lp.last_updated_ledger AS last_updated_ledger \
             FROM liquidity_pools lp FINAL \
             WHERE 1 = 1{tvl_pred}{filters} {keyset} \
             ORDER BY last_updated_ledger {order}, pool_id {order} \
             LIMIT {limit} \
         ), \
         iss AS ( \
             SELECT id, any(account_id) AS account_id FROM accounts \
             WHERE id IN (SELECT asset_a_issuer_id FROM page WHERE asset_a_issuer_id != 0 \
                          UNION ALL SELECT asset_b_issuer_id FROM page WHERE asset_b_issuer_id != 0) \
             GROUP BY id \
         ), \
         sac AS ( \
             SELECT a.asset_code AS asset_code, a.issuer_id AS issuer_id, \
                    any(sc.contract_id) AS contract_id \
             FROM assets a JOIN soroban_contracts sc ON sc.id = a.contract_id \
             WHERE a.asset_type = 2 \
               AND (a.asset_code, a.issuer_id) IN ( \
                   SELECT asset_a_code, asset_a_issuer_id FROM page WHERE asset_a_code != '' \
                   UNION ALL SELECT asset_b_code, asset_b_issuer_id FROM page WHERE asset_b_code != '') \
             GROUP BY a.asset_code, a.issuer_id \
         ) \
         SELECT \
             lower(hex(lp.pool_id))                          AS pool_id_hex, \
             lp.asset_a_type                                 AS asset_a_type, \
             nullIf(lp.asset_a_code, '')                     AS asset_a_code, \
             nullIf(iss_a.account_id, '')                    AS asset_a_issuer, \
             nullIf(sac_a.contract_id, '')                   AS asset_a_contract_id, \
             lp.asset_b_type                                 AS asset_b_type, \
             nullIf(lp.asset_b_code, '')                     AS asset_b_code, \
             nullIf(iss_b.account_id, '')                    AS asset_b_issuer, \
             nullIf(sac_b.contract_id, '')                   AS asset_b_contract_id, \
             lp.fee_bps                                      AS fee_bps, \
             ifNull(s.created_at_ledger, lp.last_updated_ledger) AS created_at_ledger, \
             lp.last_updated_ledger                          AS cursor_ledger, \
             toInt64(ifNull(pc.participant_count, 0))        AS participant_count, \
             s.latest_ledger_sequence                        AS latest_snapshot_ledger, \
             toString(s.reserve_a)                           AS reserve_a, \
             toString(s.reserve_b)                           AS reserve_b, \
             toString(s.total_shares)                        AS total_shares, \
             toString(s.tvl)                                 AS tvl, \
             toString(s.volume)                              AS volume, \
             toString(s.fee_revenue)                         AS fee_revenue, \
             if(s.latest_ledger_sequence IS NULL, NULL, \
                toUnixTimestamp64Milli(l_snap.closed_at))    AS latest_snapshot_at_ms \
         FROM page lp \
         LEFT JOIN ( \
             SELECT pool_id, \
                 toNullable(max(ledger_sequence))               AS latest_ledger_sequence, \
                 argMax(toNullable(reserve_a), ledger_sequence) AS reserve_a, \
                 argMax(toNullable(reserve_b), ledger_sequence) AS reserve_b, \
                 argMax(toNullable(total_shares), ledger_sequence) AS total_shares, \
                 argMax(tvl, ledger_sequence)                   AS tvl, \
                 argMax(volume, ledger_sequence)                AS volume, \
                 argMax(fee_revenue, ledger_sequence)           AS fee_revenue, \
                 toNullable(min(ledger_sequence))               AS created_at_ledger \
             FROM liquidity_pool_snapshots FINAL \
             WHERE pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) s ON s.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, count() AS participant_count FROM lp_positions FINAL \
             WHERE shares > 0 AND pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) pc ON pc.pool_id = lp.pool_id \
         LEFT JOIN iss iss_a ON iss_a.id = lp.asset_a_issuer_id \
         LEFT JOIN iss iss_b ON iss_b.id = lp.asset_b_issuer_id \
         LEFT JOIN sac sac_a ON sac_a.asset_code = lp.asset_a_code \
                            AND sac_a.issuer_id = lp.asset_a_issuer_id \
                            AND lp.asset_a_code != '' \
         LEFT JOIN sac sac_b ON sac_b.asset_code = lp.asset_b_code \
                            AND sac_b.issuer_id = lp.asset_b_issuer_id \
                            AND lp.asset_b_code != '' \
         LEFT JOIN ledgers l_snap ON l_snap.sequence = s.latest_ledger_sequence \
         ORDER BY lp.last_updated_ledger {order}, lp.pool_id {order}",
        tvl_cte = tvl_cte,
        tvl_pred = tvl_pred,
        filters = filters,
        keyset = keyset,
        order = order,
        limit = params.limit,
    );

    let mut query = client.query(&sql);
    for b in &binds {
        query = query.bind(b.as_str());
    }
    let rows = query.fetch_all::<PoolListChRow>().await?;

    Ok(rows
        .into_iter()
        .map(|r| PoolRow {
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
            cursor_ledger: r.cursor_ledger,
            participant_count: r.participant_count,
            latest_snapshot_ledger: r.latest_snapshot_ledger,
            reserve_a: r.reserve_a,
            reserve_b: r.reserve_b,
            total_shares: r.total_shares,
            tvl: r.tvl,
            volume: r.volume,
            fee_revenue: r.fee_revenue,
            latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_pool_id_validation() {
        assert!(is_hex_pool_id(&"a".repeat(64)));
        assert!(is_hex_pool_id(&"0123456789abcdef".repeat(4)));
        assert!(!is_hex_pool_id(&"a".repeat(63)));
        assert!(!is_hex_pool_id(&"a".repeat(65)));
        assert!(!is_hex_pool_id(&"A".repeat(64)), "uppercase rejected");
        assert!(!is_hex_pool_id("xyz"));
        assert!(!is_hex_pool_id(&"'; DROP--".repeat(8)));
    }

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
