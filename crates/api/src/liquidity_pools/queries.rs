//! Database queries for the liquidity-pool endpoints.
//!
//! Shapes pinned to canonical SQL
//! `docs/architecture/database-schema/endpoint-queries/{18,19,20,21,23}_*.sql`.

use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::common::cursor::{Direction, direction_sql};

use super::dto::{ChartDataPoint, PoolListCursor, SharesCursor};

/// Internal row carrying both the wire-visible StrKey and the surrogate
/// `accounts.id` needed for the cursor tie-breaker. The surrogate is
/// stripped before the API response is built.
#[derive(Debug)]
pub(super) struct ParticipantRow {
    /// G-StrKey resolved via JOIN on `accounts`.
    pub account: String,
    /// `accounts.id` BIGINT — used only to encode the next cursor; not
    /// exposed in the response DTO.
    pub account_id_surrogate: i64,
    /// Numeric carried as text to preserve `NUMERIC(28,7)` precision.
    pub shares: String,
    /// `100 * shares / total_pool_shares`, NULL when the pool has no
    /// snapshot in the 7-day freshness window. Already a decimal string
    /// (NUMERIC `::TEXT`) at SELECT time so the API doesn't add an
    /// f64 round-trip.
    pub share_percentage: Option<String>,
    pub first_deposit_ledger: i64,
    pub last_updated_ledger: i64,
}

/// Look up a pool by its hex `pool_id`. Returns `Ok(true)` if the pool
/// exists and is a real (non-sentinel) pool, `Ok(false)` otherwise. Used
/// to gate 404 vs 200-empty-list on the participants / transactions /
/// chart endpoints.
///
/// `created_at_ledger > 0` filters sentinel placeholder rows emitted by
/// the persist layer during partial backfills (per ADR 0041 / task 0193):
/// these rows carry `created_at_ledger = 0` and minimum-data NULL/0
/// asset/fee fields, and should not be surfaced as real pools by any API
/// endpoint. The detection criterion is single-column and uses
/// `idx_pools_created_at_ledger` for the look-up.
pub async fn pool_exists(db: &PgPool, pool_id_hex: &str) -> Result<bool, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM liquidity_pools \
         WHERE pool_id = decode($1, 'hex') AND created_at_ledger > 0",
    )
    .bind(pool_id_hex)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

/// Fetch up to `limit + 1` participants for a pool ordered by
/// `(shares DESC, account_id DESC)`. The +1 row is the peek used by
/// `common::pagination::finalize_page` to derive the next / prev
/// cursors.
///
/// Filters `lpp.shares > 0` so withdrawn participants (zero-share rows
/// retained by persist for future-history analytics — see task 0162's
/// emerged decision #2) do not appear in the active-providers view.
/// The predicate is intentionally redundant with the partial-index
/// definition (`idx_lpp_shares … WHERE shares > 0`) but kept in the
/// SQL so the query plan remains index-eligible regardless of how
/// future planners weigh it.
///
/// `share_percentage` is computed against the latest snapshot for the
/// pool (within a 7-day freshness window) via a scalar CTE
/// (`latest_snap`) evaluated once per page and broadcast to every
/// position row through `LEFT JOIN ... ON TRUE`.
pub(super) async fn fetch_participants(
    db: &PgPool,
    pool_id_hex: &str,
    cursor: Option<&SharesCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    let (cur_shares, cur_acct): (Option<String>, Option<i64>) = match cursor {
        Some(c) => (Some(c.shares.clone()), Some(c.account_id)),
        None => (None, None),
    };
    let (op, order) = direction_sql(direction);

    let sql = format!(
        r#"
        WITH latest_snap AS (
            SELECT lps.total_shares
              FROM liquidity_pool_snapshots lps
             WHERE lps.pool_id = decode($1, 'hex')
               AND lps.created_at >= NOW() - INTERVAL '7 days'
             ORDER BY lps.created_at DESC, lps.ledger_sequence DESC
             LIMIT 1
        )
        SELECT
            acc.account_id                  AS account,
            lpp.account_id                  AS account_id_surrogate,
            lpp.shares::TEXT                AS shares,
            CASE
                WHEN snap.total_shares IS NULL OR snap.total_shares = 0 THEN NULL
                ELSE (lpp.shares * 100.0 / snap.total_shares)::TEXT
            END                             AS share_percentage,
            lpp.first_deposit_ledger,
            lpp.last_updated_ledger
          FROM lp_positions lpp
          JOIN accounts acc           ON acc.id = lpp.account_id
          LEFT JOIN latest_snap snap  ON TRUE
         WHERE lpp.pool_id = decode($1, 'hex')
           AND lpp.shares > 0
           AND EXISTS (
               SELECT 1 FROM liquidity_pools lp
                WHERE lp.pool_id = decode($1, 'hex')
                  AND lp.created_at_ledger > 0
           )
           AND ($3::numeric IS NULL
                OR (lpp.shares, lpp.account_id) {op} ($3::numeric, $4::BIGINT))
         ORDER BY lpp.shares {order}, lpp.account_id {order}
         LIMIT $2
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(pool_id_hex)
        .bind(limit)
        .bind(cur_shares)
        .bind(cur_acct)
        .fetch_all(db)
        .await?;

    Ok(rows.iter().map(map_participant_row).collect())
}

fn map_participant_row(r: &PgRow) -> ParticipantRow {
    ParticipantRow {
        account: r.get("account"),
        account_id_surrogate: r.get("account_id_surrogate"),
        shares: r.get("shares"),
        share_percentage: r.get("share_percentage"),
        first_deposit_ledger: r.get("first_deposit_ledger"),
        last_updated_ledger: r.get("last_updated_ledger"),
    }
}

// ---------------------------------------------------------------------------
// List / Detail / Transactions / Chart (task 0052)
// ---------------------------------------------------------------------------

/// Canonical column projection shared between list and detail. Matches
/// `18_get_liquidity_pools_list.sql` / `19_get_liquidity_pools_by_id.sql`.
#[derive(Debug, Clone)]
pub struct PoolRow {
    pub pool_id_hex: String,
    pub asset_a_type: i16,
    pub asset_a_type_name: Option<String>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    /// C-strkey of the SAC mirror for the asset-A leg, when an
    /// `assets` row with `asset_type = 2` exists for
    /// `(asset_a_code, asset_a_issuer_id)`. `None` otherwise. Task 0263.
    pub asset_a_contract_id: Option<String>,
    /// `icon_url` from the asset-A leg's `assets` row (classic or SAC).
    /// `None` for native legs / un-enriched assets.
    pub asset_a_icon_url: Option<String>,
    pub asset_b_type: i16,
    pub asset_b_type_name: Option<String>,
    pub asset_b_code: Option<String>,
    pub asset_b_issuer: Option<String>,
    /// C-strkey of the SAC mirror for the asset-B leg. See `asset_a_contract_id`.
    pub asset_b_contract_id: Option<String>,
    /// `icon_url` from the asset-B leg's `assets` row. See `asset_a_icon_url`.
    pub asset_b_icon_url: Option<String>,
    pub fee_bps: i32,
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// `COUNT(*) FROM lp_positions WHERE pool_id = lp.pool_id AND shares > 0`.
    /// Task 0246 — see DTO doc for surfacing rules.
    pub participant_count: i64,
    pub latest_snapshot_ledger: Option<i64>,
    pub reserve_a: Option<String>,
    pub reserve_b: Option<String>,
    pub total_shares: Option<String>,
    pub tvl: Option<String>,
    pub volume: Option<String>,
    pub fee_revenue: Option<String>,
    pub latest_snapshot_at: Option<DateTime<Utc>>,
}

fn map_pool_row(r: &PgRow) -> PoolRow {
    PoolRow {
        pool_id_hex: r.get("pool_id_hex"),
        asset_a_type: r.get("asset_a_type"),
        asset_a_type_name: r.get("asset_a_type_name"),
        asset_a_code: r.get("asset_a_code"),
        asset_a_issuer: r.get("asset_a_issuer"),
        asset_a_contract_id: r.get("asset_a_contract_id"),
        asset_a_icon_url: r.get("asset_a_icon_url"),
        asset_b_type: r.get("asset_b_type"),
        asset_b_type_name: r.get("asset_b_type_name"),
        asset_b_code: r.get("asset_b_code"),
        asset_b_issuer: r.get("asset_b_issuer"),
        asset_b_contract_id: r.get("asset_b_contract_id"),
        asset_b_icon_url: r.get("asset_b_icon_url"),
        fee_bps: r.get("fee_bps"),
        fee_percent: r.get("fee_percent"),
        created_at_ledger: r.get("created_at_ledger"),
        participant_count: r.get("participant_count"),
        latest_snapshot_ledger: r.get("latest_snapshot_ledger"),
        reserve_a: r.get("reserve_a"),
        reserve_b: r.get("reserve_b"),
        total_shares: r.get("total_shares"),
        tvl: r.get("tvl"),
        volume: r.get("volume"),
        fee_revenue: r.get("fee_revenue"),
        latest_snapshot_at: r.get("latest_snapshot_at"),
    }
}

pub struct ResolvedPoolListParams {
    pub limit: i64,
    pub cursor: Option<PoolListCursor>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    pub asset_b_code: Option<String>,
    pub asset_b_issuer: Option<String>,
    /// Decimal string preserving NUMERIC(28,7) precision; passed straight
    /// to `$8::numeric` in the SQL (Postgres parses).
    pub min_tvl: Option<String>,
    /// Single-asset filter (task 0246) — trimmed + uppercased at the
    /// handler boundary, matched against either `asset_a_code` or
    /// `asset_b_code` case-insensitively (`UPPER(...) = $9`). NULL =
    /// no filter.
    pub asset_code: Option<String>,
}

pub async fn fetch_pool_list(
    pool: &PgPool,
    params: &ResolvedPoolListParams,
    direction: Direction,
) -> Result<Vec<PoolRow>, sqlx::Error> {
    let (cur_ledger, cur_pool_hex): (Option<i64>, Option<String>) = match &params.cursor {
        Some(c) => (Some(c.created_at_ledger), Some(c.pool_id_hex.clone())),
        None => (None, None),
    };
    let (op, order) = direction_sql(direction);

    let sql = format!(
        r#"
        WITH issuer_a AS (
            SELECT id FROM accounts WHERE $5::varchar IS NOT NULL AND account_id = $5
        ),
        issuer_b AS (
            SELECT id FROM accounts WHERE $7::varchar IS NOT NULL AND account_id = $7
        )
        SELECT
            encode(lp.pool_id, 'hex')           AS pool_id_hex,
            asset_type_name(lp.asset_a_type)    AS asset_a_type_name,
            lp.asset_a_type                     AS asset_a_type,
            lp.asset_a_code,
            iss_a.account_id                    AS asset_a_issuer,
            -- Task 0263: SAC mirror look-up. Pool legs only carry classic
            -- XDR `AssetType` so SAC / Soroban legs are not directly
            -- representable; this surfaces the C-strkey of an SAC mirror
            -- when one exists in `assets` for the classic credit identity
            -- `(code, issuer_id)`. NULL when no SAC mirror is registered.
            sac_a.contract_id                   AS asset_a_contract_id,
            sac_a_row.icon_url                  AS asset_a_icon_url,
            asset_type_name(lp.asset_b_type)    AS asset_b_type_name,
            lp.asset_b_type                     AS asset_b_type,
            lp.asset_b_code,
            iss_b.account_id                    AS asset_b_issuer,
            sac_b.contract_id                   AS asset_b_contract_id,
            sac_b_row.icon_url                  AS asset_b_icon_url,
            lp.fee_bps,
            (lp.fee_bps::numeric / 100)::text   AS fee_percent,
            lp.created_at_ledger,
            -- Task 0246: active liquidity providers. Correlated subquery
            -- hits the partial index `idx_lpp_shares (pool_id, shares DESC)
            -- WHERE shares > 0` once per pool row (page size = limit + 1).
            -- Not snapshot-bound, so populated even on stale pools.
            (SELECT COUNT(*) FROM lp_positions lpp
              WHERE lpp.pool_id = lp.pool_id AND lpp.shares > 0)
                                                AS participant_count,
            s.ledger_sequence                   AS latest_snapshot_ledger,
            s.reserve_a::text                   AS reserve_a,
            s.reserve_b::text                   AS reserve_b,
            s.total_shares::text                AS total_shares,
            s.tvl::text                         AS tvl,
            s.volume::text                      AS volume,
            s.fee_revenue::text                 AS fee_revenue,
            s.created_at                        AS latest_snapshot_at
        FROM liquidity_pools lp
        LEFT JOIN accounts iss_a ON iss_a.id = lp.asset_a_issuer_id
        LEFT JOIN accounts iss_b ON iss_b.id = lp.asset_b_issuer_id
        -- Per-leg `assets` row look-up, one per leg via the unique index
        -- `uidx_assets_classic_asset (asset_code, issuer_id) WHERE asset_type IN (1, 2)`
        -- (so at most one row matches — classic OR SAC). Serves two columns:
        --   * `icon_url` for the leg avatar (task 0274 gap #5), present on
        --     classic and SAC rows alike, hence `asset_type IN (1, 2)`;
        --   * the SAC mirror C-strkey (task 0263) via the onward join to
        --     `soroban_contracts` — `contract_id` is NULL on classic rows,
        --     so `asset_*_contract_id` stays NULL for non-SAC legs.
        LEFT JOIN assets sac_a_row
               ON sac_a_row.asset_code = lp.asset_a_code
              AND sac_a_row.issuer_id  = lp.asset_a_issuer_id
              AND sac_a_row.asset_type IN (1, 2)
        LEFT JOIN soroban_contracts sac_a
               ON sac_a.id = sac_a_row.contract_id
        LEFT JOIN assets sac_b_row
               ON sac_b_row.asset_code = lp.asset_b_code
              AND sac_b_row.issuer_id  = lp.asset_b_issuer_id
              AND sac_b_row.asset_type IN (1, 2)
        LEFT JOIN soroban_contracts sac_b
               ON sac_b.id = sac_b_row.contract_id
        LEFT JOIN LATERAL (
            SELECT
                lps.ledger_sequence,
                lps.reserve_a,
                lps.reserve_b,
                lps.total_shares,
                lps.tvl,
                lps.volume,
                lps.fee_revenue,
                lps.created_at
            FROM liquidity_pool_snapshots lps
            WHERE lps.pool_id = lp.pool_id
            ORDER BY lps.created_at DESC, lps.ledger_sequence DESC
            LIMIT 1
        ) s ON TRUE
        WHERE
            -- Filter sentinel placeholder pools (ADR 0041 / task 0193).
            -- Pubnet genesis seq is 1, so `created_at_ledger > 0` excludes
            -- every sentinel row without rejecting any real pool.
            lp.created_at_ledger > 0
            AND ($2::bigint IS NULL
             OR (lp.created_at_ledger, lp.pool_id) {op} ($2, decode($3::varchar, 'hex')))
            AND ($4::varchar IS NULL OR lp.asset_a_code = $4)
            AND ($5::varchar IS NULL OR lp.asset_a_issuer_id = (SELECT id FROM issuer_a))
            AND ($6::varchar IS NULL OR lp.asset_b_code = $6)
            AND ($7::varchar IS NULL OR lp.asset_b_issuer_id = (SELECT id FROM issuer_b))
            AND ($8::numeric IS NULL OR s.tvl >= $8::numeric)
            -- Task 0246: single-asset filter. `$9` is already trimmed +
            -- uppercased at the handler boundary; `UPPER(...)` on the
            -- column side covers mixed-case stored codes. The two
            -- `idx_pools_asset_a/b` btree indexes are on the *raw*
            -- column so this clause forfeits index lookup — acceptable
            -- because the planner can still seek on per-leg filters /
            -- cursor predicate when present, and a full pool-table scan
            -- is bounded (current Stellar pubnet ≈ 10⁴ pools).
            AND ($9::varchar IS NULL
                 OR UPPER(lp.asset_a_code) = $9
                 OR UPPER(lp.asset_b_code) = $9)
        ORDER BY lp.created_at_ledger {order}, lp.pool_id {order}
        LIMIT $1
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(params.limit)
        .bind(cur_ledger)
        .bind(cur_pool_hex)
        .bind(&params.asset_a_code)
        .bind(&params.asset_a_issuer)
        .bind(&params.asset_b_code)
        .bind(&params.asset_b_issuer)
        .bind(&params.min_tvl)
        .bind(&params.asset_code)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(map_pool_row).collect())
}

/// `GET /v1/liquidity-pools/:id`. Returns `Ok(None)` for missing pools so
/// the handler can surface 404; database errors propagate as
/// `Err(sqlx::Error)`.
pub async fn fetch_pool_by_id(
    pool: &PgPool,
    pool_id_hex: &str,
) -> Result<Option<PoolRow>, sqlx::Error> {
    let row: Option<PgRow> = sqlx::query(
        r#"
        SELECT
            encode(lp.pool_id, 'hex')          AS pool_id_hex,
            asset_type_name(lp.asset_a_type)   AS asset_a_type_name,
            lp.asset_a_type                    AS asset_a_type,
            lp.asset_a_code,
            iss_a.account_id                   AS asset_a_issuer,
            -- Task 0263 (contract_id) + 0274 gap #5 (icon_url): see the
            -- per-leg `assets` look-up in `fetch_pool_list`.
            sac_a.contract_id                  AS asset_a_contract_id,
            sac_a_row.icon_url                 AS asset_a_icon_url,
            asset_type_name(lp.asset_b_type)   AS asset_b_type_name,
            lp.asset_b_type                    AS asset_b_type,
            lp.asset_b_code,
            iss_b.account_id                   AS asset_b_issuer,
            sac_b.contract_id                  AS asset_b_contract_id,
            sac_b_row.icon_url                 AS asset_b_icon_url,
            lp.fee_bps,
            (lp.fee_bps::numeric / 100)::text  AS fee_percent,
            lp.created_at_ledger,
            -- Task 0246: active LP count. Same correlated subquery as
            -- file 18 (list); the partial index `idx_lpp_shares` covers
            -- it. Populated even on stale pools (no snapshot dep).
            (SELECT COUNT(*) FROM lp_positions lpp
              WHERE lpp.pool_id = lp.pool_id AND lpp.shares > 0)
                                               AS participant_count,
            s.ledger_sequence                  AS latest_snapshot_ledger,
            s.reserve_a::text                  AS reserve_a,
            s.reserve_b::text                  AS reserve_b,
            s.total_shares::text               AS total_shares,
            s.tvl::text                        AS tvl,
            s.volume::text                     AS volume,
            s.fee_revenue::text                AS fee_revenue,
            s.created_at                       AS latest_snapshot_at
        FROM liquidity_pools lp
        LEFT JOIN accounts iss_a ON iss_a.id = lp.asset_a_issuer_id
        LEFT JOIN accounts iss_b ON iss_b.id = lp.asset_b_issuer_id
        LEFT JOIN assets sac_a_row
               ON sac_a_row.asset_code = lp.asset_a_code
              AND sac_a_row.issuer_id  = lp.asset_a_issuer_id
              AND sac_a_row.asset_type IN (1, 2)
        LEFT JOIN soroban_contracts sac_a
               ON sac_a.id = sac_a_row.contract_id
        LEFT JOIN assets sac_b_row
               ON sac_b_row.asset_code = lp.asset_b_code
              AND sac_b_row.issuer_id  = lp.asset_b_issuer_id
              AND sac_b_row.asset_type IN (1, 2)
        LEFT JOIN soroban_contracts sac_b
               ON sac_b.id = sac_b_row.contract_id
        LEFT JOIN LATERAL (
            SELECT
                lps.ledger_sequence,
                lps.reserve_a,
                lps.reserve_b,
                lps.total_shares,
                lps.tvl,
                lps.volume,
                lps.fee_revenue,
                lps.created_at
            FROM liquidity_pool_snapshots lps
            WHERE lps.pool_id = lp.pool_id
            ORDER BY lps.created_at DESC, lps.ledger_sequence DESC
            LIMIT 1
        ) s ON TRUE
        WHERE lp.pool_id = decode($1::varchar, 'hex')
          -- Sentinel placeholder pools (ADR 0041 / task 0193) carry
          -- `created_at_ledger = 0`; `> 0` excludes them so this
          -- look-up returns `None` and the handler surfaces 404.
          AND lp.created_at_ledger > 0
        "#,
    )
    .bind(pool_id_hex)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(map_pool_row))
}

/// `GET /v1/liquidity-pools/:id/transactions` row. Mirrors
/// canonical SQL `20_get_liquidity_pools_transactions.sql`.
#[derive(Debug, Clone)]
pub struct PoolTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub fee_charged: i64,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn fetch_pool_transactions(
    pool: &PgPool,
    pool_id_hex: &str,
    limit: i64,
    cursor: Option<&crate::common::cursor::TsIdCursor>,
    direction: Direction,
) -> Result<Vec<PoolTxRow>, sqlx::Error> {
    let (cur_ts, cur_id): (Option<DateTime<Utc>>, Option<i64>) = match cursor {
        Some(c) => (Some(c.ts), Some(c.id)),
        None => (None, None),
    };
    let (op, order) = direction_sql(direction);

    let sql = format!(
        r#"
        -- `matched_ops` deduplicates multi-op-touching-same-pool
        -- transactions to one row per (created_at, transaction_id) via
        -- DISTINCT ON. Pre-LIMIT to `$2 * 4` is the canonical pattern
        -- shared with `02_get_transactions_list.sql` (Statement B/C) and
        -- `10_get_assets_transactions.sql`: gives the planner headroom on
        -- high-traffic pools where a small LIMIT can flip the plan to a
        -- worse shape (bitmap vs index scan). Outer SELECT then sees a
        -- small de-duplicated set instead of asking DISTINCT to dedupe
        -- after the join. See `assets/queries.rs::fetch_transactions`
        -- module doc + lore-0049 archive note.
        WITH matched_ops AS (
            SELECT DISTINCT ON (oa.created_at, oa.transaction_id)
                oa.transaction_id,
                oa.created_at,
                oa.id AS op_appearance_id
            FROM operations_appearances oa
            WHERE oa.pool_id = decode($1::varchar, 'hex')
              -- Sentinel filter (ADR 0041 / task 0193). Defense-in-depth
              -- alongside the handler-level `pool_exists()` gate: an
              -- EXISTS guard at the query body protects future callers
              -- that bypass the handler. Sentinels have no
              -- `operations_appearances` rows by construction, so this
              -- is belt-and-suspenders but cheap (uncorrelated PK seek).
              AND EXISTS (
                  SELECT 1 FROM liquidity_pools lp
                   WHERE lp.pool_id = decode($1::varchar, 'hex')
                     AND lp.created_at_ledger > 0
              )
              AND ($3::timestamptz IS NULL
                   OR (oa.created_at, oa.transaction_id) {op} ($3, $4))
            ORDER BY oa.created_at {order}, oa.transaction_id {order}, oa.id
            LIMIT $2 * 4
        )
        SELECT
            t.id                    AS id,
            encode(t.hash, 'hex')   AS hash,
            t.ledger_sequence,
            src.account_id          AS source_account,
            t.fee_charged,
            t.successful,
            t.operation_count,
            t.has_soroban,
            COALESCE(ops.operation_types, ARRAY[]::text[]) AS operation_types,
            t.created_at
        FROM matched_ops m
        JOIN transactions t
               ON t.id         = m.transaction_id
              AND t.created_at = m.created_at
        JOIN accounts src ON src.id = t.source_id
        LEFT JOIN LATERAL (
            SELECT array_agg(DISTINCT op_type_name(oa.type)
                             ORDER BY op_type_name(oa.type)) AS operation_types
            FROM operations_appearances oa
            WHERE oa.transaction_id = t.id
              AND oa.created_at     = t.created_at
        ) ops ON TRUE
        ORDER BY t.created_at {order}, t.id {order}
        LIMIT $2
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(pool_id_hex)
        .bind(limit)
        .bind(cur_ts)
        .bind(cur_id)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| PoolTxRow {
            id: r.get("id"),
            hash: r.get("hash"),
            ledger_sequence: r.get("ledger_sequence"),
            source_account: r.get("source_account"),
            fee_charged: r.get("fee_charged"),
            successful: r.get("successful"),
            operation_count: r.get("operation_count"),
            has_soroban: r.get("has_soroban"),
            operation_types: r.get("operation_types"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// `GET /v1/liquidity-pools/:id/chart`. The interval string is validated
/// by the handler against the `1h | 1d | 1w` allowlist before this is
/// called — the `assert!` below is a defensive second gate.
///
/// `assert!` (not `debug_assert!`) so a release build also panics on
/// allowlist drift instead of silently producing a NULL `bucket` (which
/// would then panic at `r.get::<DateTime<Utc>, _>("bucket")` when sqlx
/// tries to decode NULL into a non-Optional `DateTime`). Cheaper to
/// fail loud on the SQL parameter than at row decode.
pub async fn fetch_pool_chart(
    pool: &PgPool,
    pool_id_hex: &str,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ChartDataPoint>, sqlx::Error> {
    assert!(
        matches!(interval, "1h" | "1d" | "1w"),
        "fetch_pool_chart called with non-allowlisted interval `{interval}` — \
         handler validation drift; expected 1h | 1d | 1w"
    );
    let rows = sqlx::query(
        r#"
        WITH bucket_keyword AS (
            SELECT CASE $2
                WHEN '1h' THEN 'hour'
                WHEN '1d' THEN 'day'
                WHEN '1w' THEN 'week'
            END AS kw
        )
        SELECT
            date_trunc((SELECT kw FROM bucket_keyword), lps.created_at) AS bucket,
            (
                ARRAY_AGG(lps.tvl ORDER BY lps.created_at DESC, lps.ledger_sequence DESC)
            )[1]::text                  AS tvl,
            SUM(lps.volume)::text       AS volume,
            SUM(lps.fee_revenue)::text  AS fee_revenue,
            COUNT(*)                    AS samples_in_bucket
        FROM liquidity_pool_snapshots lps
        WHERE lps.pool_id     = decode($1::varchar, 'hex')
          AND lps.created_at >= $3
          AND lps.created_at <  $4
          -- Sentinel filter (ADR 0041 / task 0193). Defense-in-depth
          -- alongside the handler-level `pool_exists()` gate. Sentinels
          -- have no snapshots by construction, but the guard protects
          -- callers that bypass the handler.
          AND EXISTS (
              SELECT 1 FROM liquidity_pools lp
               WHERE lp.pool_id = decode($1::varchar, 'hex')
                 AND lp.created_at_ledger > 0
          )
        GROUP BY date_trunc((SELECT kw FROM bucket_keyword), lps.created_at)
        ORDER BY bucket ASC
        "#,
    )
    .bind(pool_id_hex)
    .bind(interval)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ChartDataPoint {
            bucket: r.get("bucket"),
            tvl: r.get("tvl"),
            volume: r.get("volume"),
            fee_revenue: r.get("fee_revenue"),
            samples_in_bucket: r.get("samples_in_bucket"),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Sentinel-filter direct query tests (ADR 0041 / task 0193)
//
// The integration tests in `tests_integration.rs` exercise the full
// handler stack, where `pool_exists()` short-circuits sentinel pool ids
// to 404 *before* the per-endpoint query runs. That means the
// defense-in-depth `EXISTS` guards inside `fetch_participants`,
// `fetch_pool_transactions`, and `fetch_pool_chart` never fire in those
// tests — they have zero coverage from the handler path.
//
// These tests call those three functions directly with a sentinel
// `pool_id` against a fixture that seeds *real* supporting rows
// (lp_position, snapshot, op_appearance) for the sentinel pool. Without
// the EXISTS guard, each function would return non-empty results. With
// the guard, each returns an empty `Vec`. The assertion proves the SQL
// predicate fires — not just the handler-level gate.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod sentinel_query_tests {
    use super::*;
    use chrono::{Duration, Utc};

    const SENTINEL_HEX: &str = "7777666655554444333322221111000099998888aaaabbbbccccddddeeeeffff";
    const ACC: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0193QRY";
    const TX_HASH_HEX: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef01930001";

    /// Try to obtain a connection. Returns `None` when `DATABASE_URL` is
    /// unset / unreachable — the test then skips cleanly, matching the
    /// pattern used elsewhere in this crate's integration tests.
    async fn try_connect() -> Option<PgPool> {
        let url = std::env::var("DATABASE_URL").ok()?;
        match PgPool::connect(&url).await {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("DATABASE_URL unreachable ({err}) — skipping sentinel query test");
                None
            }
        }
    }

    /// Seed: sentinel pool + 1 account + 1 lp_position + 1 snapshot +
    /// 1 transaction + 1 op_appearance pointing at the sentinel pool.
    /// All supporting rows reference the sentinel `pool_id` so the
    /// per-endpoint queries *would* return non-empty results if the
    /// EXISTS guard were missing.
    ///
    /// Returns the account surrogate id (kept for symmetry; not used by
    /// the assertions but required for downstream FK).
    async fn seed(pool: &PgPool) -> i64 {
        // Sentinel pool (created_at_ledger = 0 — the marker).
        sqlx::query(
            r#"
            INSERT INTO liquidity_pools (
                pool_id, asset_a_type, asset_a_code, asset_a_issuer_id,
                asset_b_type, asset_b_code, asset_b_issuer_id,
                fee_bps, created_at_ledger
            ) VALUES (decode($1, 'hex'), 0, NULL, NULL, 0, NULL, NULL, 0, 0)
            "#,
        )
        .bind(SENTINEL_HEX)
        .execute(pool)
        .await
        .expect("insert sentinel pool");

        let acc_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
               VALUES ($1, 1, 1, 0) RETURNING id"#,
        )
        .bind(ACC)
        .fetch_one(pool)
        .await
        .expect("insert acc");

        // Position would be returned by `fetch_participants` without the
        // guard.
        sqlx::query(
            r#"
            INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger)
            VALUES (decode($1, 'hex'), $2, 42.0::NUMERIC(28,7), 1, 1)
            "#,
        )
        .bind(SENTINEL_HEX)
        .bind(acc_id)
        .execute(pool)
        .await
        .expect("insert lp_position");

        // Snapshot would surface as a chart bucket without the guard.
        // `created_at = NOW()` lands in the live `_default` partition.
        sqlx::query(
            r#"
            INSERT INTO liquidity_pool_snapshots (
                pool_id, ledger_sequence, reserve_a, reserve_b, total_shares, created_at
            )
            VALUES (decode($1, 'hex'), 1, 100.0, 200.0, 42.0, NOW())
            "#,
        )
        .bind(SENTINEL_HEX)
        .execute(pool)
        .await
        .expect("insert snapshot");

        // Transaction + op_appearance: without the EXISTS guard,
        // `fetch_pool_transactions` would return this tx.
        sqlx::query(
            r#"
            INSERT INTO transactions (
                hash, ledger_sequence, application_order, source_id,
                fee_charged, successful, operation_count, has_soroban,
                parse_error, created_at
            ) VALUES (decode($1, 'hex'), 1, 1, $2, 100, true, 1, false, false, NOW())
            "#,
        )
        .bind(TX_HASH_HEX)
        .bind(acc_id)
        .execute(pool)
        .await
        .expect("insert transaction");

        sqlx::query(
            r#"
            INSERT INTO operations_appearances (
                transaction_id, type, source_id, pool_id,
                amount, ledger_sequence, created_at
            )
            SELECT t.id, 22, $2, decode($1, 'hex'), 1, 1, t.created_at
              FROM transactions t
             WHERE t.hash = decode($3, 'hex')
             LIMIT 1
            "#,
        )
        .bind(SENTINEL_HEX)
        .bind(acc_id)
        .bind(TX_HASH_HEX)
        .execute(pool)
        .await
        .expect("insert op_appearance");

        acc_id
    }

    async fn teardown(pool: &PgPool) {
        // Order matters: drop dependents (op_appearance, tx, snapshot,
        // position) before the pool / account.
        let _ = sqlx::query("DELETE FROM operations_appearances WHERE pool_id = decode($1, 'hex')")
            .bind(SENTINEL_HEX)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM transactions WHERE hash = decode($1, 'hex')")
            .bind(TX_HASH_HEX)
            .execute(pool)
            .await;
        let _ =
            sqlx::query("DELETE FROM liquidity_pool_snapshots WHERE pool_id = decode($1, 'hex')")
                .bind(SENTINEL_HEX)
                .execute(pool)
                .await;
        let _ = sqlx::query("DELETE FROM lp_positions WHERE pool_id = decode($1, 'hex')")
            .bind(SENTINEL_HEX)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM liquidity_pools WHERE pool_id = decode($1, 'hex')")
            .bind(SENTINEL_HEX)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM accounts WHERE account_id = $1")
            .bind(ACC)
            .execute(pool)
            .await;
    }

    /// All three EXISTS-guarded fetchers must return an empty `Vec` for
    /// a sentinel `pool_id`, *even though* the underlying tables
    /// (lp_positions, liquidity_pool_snapshots, operations_appearances)
    /// carry rows that would otherwise match. Proves the SQL-level
    /// guard fires independently of the handler-level `pool_exists()`
    /// gate.
    #[tokio::test]
    async fn sentinel_pool_id_returns_empty_from_all_three_fetchers() {
        let Some(pool) = try_connect().await else {
            return;
        };

        // Idempotent setup.
        teardown(&pool).await;
        seed(&pool).await;

        // 1) fetch_participants — would surface the lp_position row.
        let parts = fetch_participants(&pool, SENTINEL_HEX, None, 10, Direction::Next)
            .await
            .expect("fetch_participants for sentinel");
        assert!(
            parts.is_empty(),
            "fetch_participants leaked {} row(s) for sentinel pool — EXISTS guard not firing",
            parts.len()
        );

        // 2) fetch_pool_chart — would surface a bucket from the
        //    seeded snapshot. Use a wide window so any `created_at=NOW()`
        //    snapshot falls inside.
        let to = Utc::now() + Duration::days(1);
        let from = to - Duration::days(7);
        let buckets = fetch_pool_chart(&pool, SENTINEL_HEX, "1d", from, to)
            .await
            .expect("fetch_pool_chart for sentinel");
        assert!(
            buckets.is_empty(),
            "fetch_pool_chart leaked {} bucket(s) for sentinel pool — EXISTS guard not firing",
            buckets.len()
        );

        // 3) fetch_pool_transactions — would surface the seeded tx.
        let txs = fetch_pool_transactions(&pool, SENTINEL_HEX, 10, None, Direction::Next)
            .await
            .expect("fetch_pool_transactions for sentinel");
        assert!(
            txs.is_empty(),
            "fetch_pool_transactions leaked {} row(s) for sentinel pool — EXISTS guard not firing",
            txs.len()
        );

        teardown(&pool).await;
    }
}
