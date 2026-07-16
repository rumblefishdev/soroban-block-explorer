//! ClickHouse queries for the liquidity-pool endpoints (task 0243).
//!
//! Returns the `PoolRow` / `PoolTxRow` /
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

use crate::common::ch::{fetch_tx_list_aggregates, millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::transactions::dto::TxListCursor;

use super::dto::{ChartDataPoint, PoolListCursor, SharesCursor};

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

/// Canonical pool column projection shared between list and detail.
#[derive(Debug, Clone)]
pub struct PoolRow {
    pub pool_id_hex: String,
    pub asset_a_type: i16,
    pub asset_a_type_name: Option<String>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    /// C-strkey of the SAC mirror for the asset-A leg. `None` otherwise (task 0263).
    pub asset_a_contract_id: Option<String>,
    /// `icon_url` from the asset-A leg's `assets` row (classic or SAC).
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
    /// Ledger value the list keyset orders + paginates on. CH keys on the
    /// native `last_updated_ledger` ("most recently active"), carried here.
    /// The wire `PoolListCursor.created_at_ledger` slot stays opaque (ADR
    /// 0008); only this field feeds the cursor builder. Unused by detail.
    pub cursor_ledger: i64,
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

/// One current LP participant (a positive-shares position). Handler strips the
/// surrogate before building the API response.
#[derive(Debug)]
pub struct ParticipantRow {
    /// G-StrKey resolved via JOIN on `accounts`.
    pub account: String,
    /// `accounts.id` BIGINT — used only to encode the next cursor; not
    /// exposed in the response DTO.
    pub account_id_surrogate: i64,
    /// Numeric carried as text to preserve `NUMERIC(28,7)` precision.
    pub shares: String,
    /// `100 * shares / total_pool_shares`, NULL when the pool has no snapshot
    /// in the 7-day freshness window. Already a decimal string.
    pub share_percentage: Option<String>,
    pub first_deposit_ledger: i64,
    pub last_updated_ledger: i64,
}

/// Resolved, validated `GET /v1/liquidity-pools` list params.
pub struct ResolvedPoolListParams {
    pub limit: i64,
    pub cursor: Option<PoolListCursor>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    pub asset_b_code: Option<String>,
    pub asset_b_issuer: Option<String>,
    /// Decimal string preserving NUMERIC(28,7) precision.
    pub min_tvl: Option<String>,
    /// Single-asset filter (task 0246) — trimmed + uppercased at the handler
    /// boundary, matched against either leg case-insensitively. NULL = no filter.
    pub asset_code: Option<String>,
}

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
    asset_a_icon_url: Option<String>,
    asset_b_type: i16,
    asset_b_code: Option<String>,
    asset_b_issuer: Option<String>,
    asset_b_contract_id: Option<String>,
    asset_b_icon_url: Option<String>,
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
    // `unhex(?)` appears 5×: the `legs` CTE, the created_at-ledger subquery, the
    // participant-count subquery, the latest-snapshot subquery, and the outer
    // WHERE. All scoped to the literal pool id (NOT correlated to `lp`) since
    // detail is single-pool and CH dislikes correlated subqueries. Each `?`
    // consumes one positional bind; all are the same value, so order is moot.
    //
    // `legs` resolves the pool's two `(code, issuer_id)` pairs once; `iss` and
    // `sac` both fan out from it.
    //
    // **Issuer resolution is a restricted `iss` CTE, NOT `accounts FINAL`
    // joins.** `accounts` is `ORDER BY (account_id)`, so the surrogate `id` is a
    // non-PK reverse lookup; a plain `LEFT JOIN accounts FINAL` builds the whole
    // 14M-row table into the hash — and detail does it for BOTH legs, blowing
    // the 3.73 GiB per-query cap (box-confirmed `Code 241`). Restricting to the
    // pool's ≤2 issuer ids + `GROUP BY id` (no FINAL — account_id is stable
    // across RMT versions, `any()` is safe) scans the id column but builds a
    // ≤2-row hash. Same shape as `fetch_pool_list`'s `iss` CTE.
    //
    // SAC mirror + icon_url (task 0263 + 0274 gap #5 → ADR 0051): the `sac` CTE
    // resolves `(asset_code, issuer_id)` → `(contract_id, icon_url)` once per leg,
    // deduped by GROUP BY so a leg cannot fan the result out (the inline-join
    // form did, masked only by the outer LIMIT 1). Post-ADR 0051 the SAC handle
    // is a FACET in the `asset_sac` side table (not a column on `assets`, and not
    // a separate `asset_type = 2`) — so the deployed SAC's `C…` StrKey resolves by
    // two hops: leg `(code, issuer)` → `asset_sac.sac_contract_id` (surrogate) →
    // `soroban_contracts.contract_id` (un-deployed SACs have no contract row →
    // NULL, as before). The classic carrier is `asset_type IN (0, 1)`. Native legs
    // (`asset_code = ''`) are excluded from the join (NULL code → no assets match →
    // NULL contract_id + NULL icon_url).
    //
    // **Latest snapshot subquery — NO `FINAL`** (0356 / PR #318). The indexer now
    // writes exactly one deterministic row per `(pool_id, ledger_sequence)`, so
    // `FINAL` is redundant for dedup; dropping it turns the read into a bounded
    // reverse-PK seek (`ORDER BY ledger_sequence DESC LIMIT 1`) instead of a
    // whole-table merge. It stays a whole-row `LIMIT 1` (not per-column
    // `argMax`), so `reserve_a`/`reserve_b` can never tear across a stale
    // before/after pair in the pre-cleanup window. `created_at_ledger` already
    // reads without `FINAL` (`min(ledger_sequence)` is dup-invariant).
    let row = client
        .query(
            "WITH legs AS ( \
                 SELECT asset_a_code, asset_a_issuer_id, asset_b_code, asset_b_issuer_id \
                 FROM liquidity_pools FINAL WHERE pool_id = unhex(?) \
             ), \
             iss AS ( \
                 SELECT id, any(account_id) AS account_id FROM accounts \
                 WHERE id IN (SELECT asset_a_issuer_id FROM legs WHERE asset_a_issuer_id != 0 \
                              UNION ALL SELECT asset_b_issuer_id FROM legs WHERE asset_b_issuer_id != 0) \
                 GROUP BY id \
             ), \
             sac AS ( \
                 SELECT a.asset_code AS asset_code, a.issuer_id AS issuer_id, \
                        max(sc.contract_id) AS contract_id, \
                        max(a.icon_url)     AS icon_url \
                 FROM assets a \
                 LEFT JOIN ( \
                     SELECT asset_type, asset_code, issuer_id, contract_id, \
                            max(sac_contract_id) AS sac_contract_id \
                     FROM asset_sac GROUP BY asset_type, asset_code, issuer_id, contract_id \
                 ) asac ON asac.asset_type = a.asset_type AND asac.asset_code = a.asset_code \
                       AND asac.issuer_id = a.issuer_id AND asac.contract_id = a.contract_id \
                 LEFT JOIN soroban_contracts sc ON sc.id = asac.sac_contract_id AND asac.sac_contract_id != 0 \
                 WHERE a.asset_type IN (0, 1) \
                   AND (a.asset_code, a.issuer_id) IN ( \
                       SELECT asset_a_code, asset_a_issuer_id FROM legs WHERE asset_a_code != '' \
                       UNION ALL \
                       SELECT asset_b_code, asset_b_issuer_id FROM legs WHERE asset_b_code != '') \
                 GROUP BY a.asset_code, a.issuer_id \
             ) \
             SELECT \
                lower(hex(lp.pool_id))               AS pool_id_hex, \
                lp.asset_a_type                      AS asset_a_type, \
                nullIf(lp.asset_a_code, '')          AS asset_a_code, \
                nullIf(iss_a.account_id, '')         AS asset_a_issuer, \
                nullIf(sac_a.contract_id, '')        AS asset_a_contract_id, \
                sac_a.icon_url                       AS asset_a_icon_url, \
                lp.asset_b_type                      AS asset_b_type, \
                nullIf(lp.asset_b_code, '')          AS asset_b_code, \
                nullIf(iss_b.account_id, '')         AS asset_b_issuer, \
                nullIf(sac_b.contract_id, '')        AS asset_b_contract_id, \
                sac_b.icon_url                       AS asset_b_icon_url, \
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
                toString(s.tvl)                      AS tvl, \
                toString(s.volume)                   AS volume, \
                toString(s.fee_revenue)              AS fee_revenue, \
                nullIf(toUnixTimestamp64Milli(l.closed_at), 0) AS latest_snapshot_at_ms \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN iss iss_a ON iss_a.id = lp.asset_a_issuer_id \
             LEFT JOIN iss iss_b ON iss_b.id = lp.asset_b_issuer_id \
             LEFT JOIN sac sac_a ON sac_a.asset_code = lp.asset_a_code \
                                AND sac_a.issuer_id = lp.asset_a_issuer_id \
                                AND lp.asset_a_code != '' \
             LEFT JOIN sac sac_b ON sac_b.asset_code = lp.asset_b_code \
                                AND sac_b.issuer_id = lp.asset_b_issuer_id \
                                AND lp.asset_b_code != '' \
             LEFT JOIN ( \
                 SELECT toNullable(ledger_sequence) AS ledger_sequence, \
                        toNullable(reserve_a)       AS reserve_a, \
                        toNullable(reserve_b)       AS reserve_b, \
                        toNullable(total_shares)    AS total_shares, \
                        tvl, volume, fee_revenue \
                 FROM liquidity_pool_snapshots \
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
        asset_a_icon_url: r.asset_a_icon_url,
        asset_b_type: r.asset_b_type,
        asset_b_type_name: asset_type_name(r.asset_b_type),
        asset_b_code: r.asset_b_code,
        asset_b_issuer: r.asset_b_issuer,
        asset_b_contract_id: r.asset_b_contract_id,
        asset_b_icon_url: r.asset_b_icon_url,
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
    let (op, order) = keyset_sql_desc(direction);

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
            lpp.account_id                       AS account_id_surrogate, \
            toString(lpp.shares)                 AS shares, \
            if(snap.ts IS NULL OR snap.ts = toDecimal128(0, 7), NULL, \
               toString(lpp.shares * 100 / snap.ts)) AS share_percentage, \
            lpp.first_deposit_ledger             AS first_deposit_ledger, \
            lpp.last_updated_ledger              AS last_updated_ledger \
         FROM lp_positions lpp FINAL \
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

    // Resolve the provider StrKey by surrogate id (bloom seek) instead of a
    // whole-`accounts` `JOIN accounts acc FINAL` (task 0354). INNER-JOIN drop
    // semantics preserved via filter_map (a position always has its account).
    let accounts = resolve_accounts(
        client,
        rows.iter().map(|r| r.account_id_surrogate).collect(),
    )
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let account = accounts.get(&r.account_id_surrogate)?.clone();
            Some(ParticipantRow {
                account,
                account_id_surrogate: r.account_id_surrogate,
                shares: r.shares,
                share_percentage: r.share_percentage,
                first_deposit_ledger: r.first_deposit_ledger,
                last_updated_ledger: r.last_updated_ledger,
            })
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct PoolTxChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    source_id: i64,
    fee_charged: i64,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at_ms: i64,
}

/// Page key (one per pool transaction) from the `operation_pools` prefix-seek driver.
#[derive(Debug, Row, Deserialize)]
struct DriverKeyRow {
    ls: i64,
    tid: i64,
}

/// `GET /v1/liquidity-pools/:id/transactions`. Mirrors the PG
/// `fetch_pool_transactions` (canonical SQL 20).
///
/// Two cost notes:
/// - **READ-COST — prefix-seek on the `operation_pools` companion (task 0365).**
///   `operations_appearances` is `ORDER BY (ledger_sequence, …)`, so the historical
///   `has(pool_ids, X)` filter was NOT a key seek: a popular pool sits in ~every
///   granule (the `idx_oa_pool_ids` bloom cannot prune), so the read-in-order page
///   driver walked back from the tip (box-measured up to 6.75B rows). `operation_pools`
///   is the pool-keyed presence twin of `transaction_participants`
///   (`ORDER BY (pool_id, ledger_sequence, transaction_id)`), so STEP 1 is a direct
///   PK prefix-seek bounded to the pool's own rows; `LIMIT 1 BY (ledger, tx)` yields
///   `limit` distinct txs with no over-fetch dance. STEP 2 enriches the ≤limit keys
///   via the transactions/accounts/ledgers joins.
/// - The cursor is the datasource-tagged [`TxListCursor`] (shared with the
///   global transactions list). The CH variant carries `ledger_sequence` +
///   `tiebreak` (= `transaction_id`) directly, so the keyset runs on
///   `(ledger_sequence, transaction_id)` with NO `closed_at` round-trip. An
///   earlier revision reconstructed the boundary ledger from the cursor's
///   `created_at` via `closed_at = fromUnixTimestamp64Milli(...) LIMIT 1`, but
///   `ledgers.closed_at` is NOT unique (Stellar close times are second-grained,
///   so same-second ledgers share it) → the `LIMIT 1` picked an arbitrary
///   ledger and dropped/duplicated a page boundary. Carrying the ledger in the
///   cursor removes that lossy step entirely. `transaction_id` == `transactions.id`.
pub async fn fetch_pool_transactions(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<PoolTxRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Keyset on `(ledger_sequence, transaction_id)`, expanded to scalar
    // comparisons. Both bounds are i64 (no injection), inlined like the other CH
    // list paths. A `Pg`-tagged cursor never reaches here (the handler rejects a
    // cross-datasource cursor via `cursor_matches_source`); treat it as no
    // keyset (first page) defensively.
    let keyset = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => {
            format!(" AND (ledger_sequence, transaction_id) {op} ({ledger_sequence}, {tiebreak})")
        }
        _ => String::new(),
    };

    // STEP 1 — leading-key seek over `operation_pools` (task 0365). `pool_id` IS
    // the sort-key prefix, so this is a direct PK prefix-seek (~page-size),
    // superseding the density-dependent read-in-order scan over
    // `operations_appearances` (0281-C): a popular pool sat in ~every granule, so
    // the `has(pool_ids, X)` filter could not prune and walked back from the tip
    // (box-measured up to 6.75B rows). Here one row per (pool, ledger, tx);
    // `LIMIT 1 BY (ledger, tx)` collapses a tx's N pool-op rows to one, so the page
    // is `limit` DISTINCT txs directly — no over-fetch + Rust-dedup dance. The
    // `max(sequence)` fence keeps the seek behind the ledgers commit marker.
    //
    // `toFixedString(unhex(?), 32)`: the validated 64-char-hex pool id → the raw 32
    // bytes matching the `pool_id FixedString(32)` key (exactly 32 bytes, so the
    // cast never pads/truncates).
    let driver_sql = format!(
        "SELECT ledger_sequence AS ls, transaction_id AS tid \
         FROM operation_pools \
         WHERE pool_id = toFixedString(unhex(?), 32) \
           AND ledger_sequence <= (SELECT max(sequence) FROM ledgers) {keyset} \
         ORDER BY ledger_sequence {order}, transaction_id {order} \
         LIMIT 1 BY ledger_sequence, transaction_id \
         LIMIT {limit}",
        keyset = keyset,
        order = order,
        limit = limit,
    );
    let keys = client
        .query(&driver_sql)
        .bind(pool_id_hex)
        .fetch_all::<DriverKeyRow>()
        .await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    // STEP 2 — enrich the bounded page keys via the transactions / accounts /
    // ledgers joins. Keys inlined (i64, no injection) with a partition prune on
    // `transactions` (PARTITION BY intDiv(ledger_sequence, 500000)) so the
    // `(ledger_sequence, id) IN (…)` filter is a tight PK seek — same shape as
    // `common::ch::fetch_tx_list_aggregates`.
    let in_tuples = keys
        .iter()
        .map(|k| format!("({},{})", k.ls, k.tid))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = keys
        .iter()
        .map(|k| k.ls / 500_000)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let detail_sql = format!(
        "SELECT \
            t.id                                 AS id, \
            lower(hex(t.hash))                   AS hash, \
            t.ledger_sequence                    AS ledger_sequence, \
            t.source_id                          AS source_id, \
            t.fee_charged                        AS fee_charged, \
            t.successful                         AS successful, \
            t.operation_count                    AS operation_count, \
            t.has_soroban                        AS has_soroban, \
            toUnixTimestamp64Milli(l.closed_at)  AS created_at_ms \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions}) \
         ORDER BY t.ledger_sequence {order}, t.id {order} \
         LIMIT 1 BY t.id \
         LIMIT {limit}",
        in_tuples = in_tuples,
        partitions = partitions,
        order = order,
        limit = limit,
    );

    let page = client.query(&detail_sql).fetch_all::<PoolTxChRow>().await?;

    if page.is_empty() {
        return Ok(Vec::new());
    }

    // operation_types via the shared non-correlated aggregate (ops-only, PK
    // seek on the page's tx keys).
    let keys: Vec<(i64, i64)> = page.iter().map(|r| (r.ledger_sequence, r.id)).collect();
    let aggregates = fetch_tx_list_aggregates(client, &keys).await?;
    // Resolve source StrKeys by surrogate id (bloom seek) instead of a
    // whole-`accounts` `INNER JOIN accounts src` (task 0354). INNER-JOIN drop
    // preserved via filter_map (a tx always has its source account).
    let accounts = resolve_accounts(client, page.iter().map(|r| r.source_id).collect()).await?;

    Ok(page
        .into_iter()
        .filter_map(|r| {
            let source_account = accounts.get(&r.source_id)?.clone();
            let operation_types = aggregates
                .get(&r.id)
                .map(|a| a.operation_types.clone())
                .unwrap_or_default();
            Some(PoolTxRow {
                id: r.id,
                hash: r.hash,
                ledger_sequence: r.ledger_sequence,
                source_account,
                fee_charged: r.fee_charged,
                successful: r.successful,
                operation_count: r.operation_count,
                has_soroban: r.has_soroban,
                operation_types,
                created_at: millis_to_utc(r.created_at_ms),
            })
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
    //
    // **NO `FINAL`** (0356 / PR #318): `sum()`/`count()` over bucketed snapshots
    // must see exactly one row per ledger, so a bare `FROM … FINAL` can't just be
    // dropped — pre-cleanup before/after duplicates would double-count volume /
    // fee_revenue / samples. Instead the inner subquery collapses to one row per
    // ledger (`LIMIT 1 BY ledger_sequence`) with no merge; tvl/volume/fee are
    // identical across a duplicate pair, so which row survives is irrelevant. The
    // outer bucket aggregation is then byte-identical to the old `FINAL` form.
    let sql = format!(
        "SELECT \
            toUnixTimestamp64Milli(toDateTime64({bucket_fn}(l.closed_at), 3, 'UTC')) AS bucket_ms, \
            toString(argMax(lps.tvl, lps.ledger_sequence)) AS tvl, \
            toString(sum(lps.volume))                      AS volume, \
            toString(sum(lps.fee_revenue))                 AS fee_revenue, \
            count()                                        AS samples_in_bucket \
         FROM ( \
             SELECT ledger_sequence, tvl, volume, fee_revenue \
             FROM liquidity_pool_snapshots \
             WHERE pool_id = unhex(?) \
             ORDER BY ledger_sequence DESC \
             LIMIT 1 BY ledger_sequence \
         ) lps \
         JOIN ledgers l ON l.sequence = lps.ledger_sequence \
         WHERE l.closed_at >= fromUnixTimestamp64Milli(?) \
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
    asset_a_issuer_id: i64,
    asset_a_contract_id: Option<String>,
    asset_a_icon_url: Option<String>,
    asset_b_type: i16,
    asset_b_code: Option<String>,
    asset_b_issuer_id: i64,
    asset_b_contract_id: Option<String>,
    asset_b_icon_url: Option<String>,
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
    let (op, order) = keyset_sql_desc(direction);

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
    // NO `FINAL` (0356 / PR #318): `tvl` is identical across a before/after
    // duplicate pair, so `argMax(tvl, ledger_sequence)` is tie-safe and the dedup
    // `FINAL` would add is pure merge overhead here.
    // The handler already validated the decimal shape; `is_decimal_str` re-guards
    // the inline. Invalid → filter skipped (handler guarantees it never is).
    let (tvl_cte, tvl_pred) = match params.min_tvl.as_deref() {
        Some(m) if is_decimal_str(m) => (
            format!(
                "tvl_pools AS ( \
                    SELECT pool_id, argMax(tvl, ledger_sequence) AS latest_tvl \
                    FROM liquidity_pool_snapshots \
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

    // Latest-snapshot + `created_at_ledger` via streaming `argMax(...)` /
    // `min(...)` GROUP BY pool_id over `liquidity_pool_snapshots FINAL`. FINAL
    // dedups the RMT before/after image pair at each `(pool_id, ledger_sequence)`
    // so per-column `argMax` can't tear `reserve_a` from `reserve_b`. Memory is
    // O(page pools): the streaming aggregate holds ~20 groups regardless of how
    // many snapshots each pool has.
    //
    // Do NOT rewrite as `ORDER BY ledger_sequence DESC LIMIT 1 BY pool_id`
    // (PR #335, reverted): `LIMIT 1 BY` is NOT a seek — it fully materialises +
    // sorts every snapshot of the page's pools (~30M rows for the busiest 20),
    // OOMing the 4 GB read-only CH profile. A future perf pass must keep the
    // O(page pools) shape (e.g. `argMax` over a whole-row tuple), not a sort.
    //
    // Aggregates wrap in `toNullable(...)` so a no-snapshot pool yields NULL (not
    // the 0/'' default) on the LEFT JOIN miss — `join_use_nulls` is rejected for
    // the read-only CH user, so this is the readonly-safe NULL path. (Every
    // current pool has ≥ 1 snapshot, so this is defensive.) `nullIf(...)` does the
    // same for the empty-string-sentinel string columns. Native legs
    // (asset_code = '') are excluded from the SAC join by the `lp.asset_*_code !=
    // ''` guard so they surface a NULL `contract_id`, matching PG (NULL code → no
    // SAC match).
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
         sac AS ( \
             SELECT a.asset_code AS asset_code, a.issuer_id AS issuer_id, \
                    max(sc.contract_id) AS contract_id, \
                    max(a.icon_url)     AS icon_url \
             FROM assets a \
             LEFT JOIN ( \
                 SELECT asset_type, asset_code, issuer_id, contract_id, \
                        max(sac_contract_id) AS sac_contract_id \
                 FROM asset_sac GROUP BY asset_type, asset_code, issuer_id, contract_id \
             ) asac ON asac.asset_type = a.asset_type AND asac.asset_code = a.asset_code \
                   AND asac.issuer_id = a.issuer_id AND asac.contract_id = a.contract_id \
             LEFT JOIN soroban_contracts sc ON sc.id = asac.sac_contract_id AND asac.sac_contract_id != 0 \
             WHERE a.asset_type IN (0, 1) \
               AND (a.asset_code, a.issuer_id) IN ( \
                   SELECT asset_a_code, asset_a_issuer_id FROM page WHERE asset_a_code != '' \
                   UNION ALL SELECT asset_b_code, asset_b_issuer_id FROM page WHERE asset_b_code != '') \
             GROUP BY a.asset_code, a.issuer_id \
         ) \
         SELECT \
             lower(hex(lp.pool_id))                          AS pool_id_hex, \
             lp.asset_a_type                                 AS asset_a_type, \
             nullIf(lp.asset_a_code, '')                     AS asset_a_code, \
             lp.asset_a_issuer_id                            AS asset_a_issuer_id, \
             nullIf(sac_a.contract_id, '')                   AS asset_a_contract_id, \
             sac_a.icon_url                                  AS asset_a_icon_url, \
             lp.asset_b_type                                 AS asset_b_type, \
             nullIf(lp.asset_b_code, '')                     AS asset_b_code, \
             lp.asset_b_issuer_id                            AS asset_b_issuer_id, \
             nullIf(sac_b.contract_id, '')                   AS asset_b_contract_id, \
             sac_b.icon_url                                  AS asset_b_icon_url, \
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
             nullIf(toUnixTimestamp64Milli(l_snap.closed_at), 0) AS latest_snapshot_at_ms \
         FROM page lp \
         LEFT JOIN ( \
             SELECT pool_id, \
                toNullable(max(ledger_sequence))                  AS latest_ledger_sequence, \
                argMax(toNullable(reserve_a), ledger_sequence)    AS reserve_a, \
                argMax(toNullable(reserve_b), ledger_sequence)    AS reserve_b, \
                argMax(toNullable(total_shares), ledger_sequence) AS total_shares, \
                argMax(tvl, ledger_sequence)                      AS tvl, \
                argMax(volume, ledger_sequence)                   AS volume, \
                argMax(fee_revenue, ledger_sequence)              AS fee_revenue, \
                toNullable(min(ledger_sequence))                  AS created_at_ledger \
             FROM liquidity_pool_snapshots FINAL \
             WHERE pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) s ON s.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, count() AS participant_count FROM lp_positions FINAL \
             WHERE shares > 0 AND pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) pc ON pc.pool_id = lp.pool_id \
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

    // Resolve issuer StrKeys by surrogate id (bloom seek). The old in-query `iss`
    // CTE used `WHERE id IN (SELECT … FROM page)` — the subquery form does not
    // trigger the `idx_acc_id` bloom, so it scanned `accounts.id` (task 0345).
    let issuer_ids = rows
        .iter()
        .flat_map(|r| [r.asset_a_issuer_id, r.asset_b_issuer_id])
        // Exclude the native sentinel `0` — the old `iss` CTE filtered
        // `WHERE … != 0`. A no-op on real data (`accounts.id = cityhash64(strkey)`
        // is never 0), but keeps the resolution unconditionally identical.
        .filter(|&id| id != 0)
        .collect();
    let accounts = resolve_accounts(client, issuer_ids).await?;

    Ok(rows
        .into_iter()
        .map(|r| PoolRow {
            pool_id_hex: r.pool_id_hex,
            asset_a_type: r.asset_a_type,
            asset_a_type_name: asset_type_name(r.asset_a_type),
            asset_a_code: r.asset_a_code,
            asset_a_issuer: accounts
                .get(&r.asset_a_issuer_id)
                .cloned()
                .filter(|s| !s.is_empty()),
            asset_a_contract_id: r.asset_a_contract_id,
            asset_a_icon_url: r.asset_a_icon_url,
            asset_b_type: r.asset_b_type,
            asset_b_type_name: asset_type_name(r.asset_b_type),
            asset_b_code: r.asset_b_code,
            asset_b_issuer: accounts
                .get(&r.asset_b_issuer_id)
                .cloned()
                .filter(|s| !s.is_empty()),
            asset_b_contract_id: r.asset_b_contract_id,
            asset_b_icon_url: r.asset_b_icon_url,
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

/// Live-CH **decode** smoke for the LP read path.
///
/// The curl `FORMAT TSV/Vertical/JSON` box smokes do NOT exercise the
/// clickhouse-rs RowBinary decoder, so a wire-type↔struct mismatch — e.g. a
/// scalar `(SELECT count() …)` typed `Nullable(UInt64)` decoded into an `i64`
/// field (the detail `participant_count` bug, task 0243) — passes a curl check
/// yet 500s the live endpoint with `schema mismatch`. A pure-Rust round-trip
/// can't catch it either (the struct serializes consistently with itself). The
/// only real guard is decoding rows that an actual CH produced.
///
/// This test runs each cheap LP CH fetch fn against a real CH and asserts the
/// rows decode (no error). It **skips cleanly when `CH_URL` is unset**, so CI
/// (no CH access) is unaffected. Run it against a reachable CH — a local
/// replica or an SSH tunnel to the box:
///
/// ```text
/// CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
///   cargo test -p api --lib decode_smoke -- --nocapture
/// ```
///
/// `transactions` is intentionally excluded: its driver scans the whole
/// `operations_appearances` table (~7.87B rows) until the `pool_id` projection
/// lands, so exercising it here would blow the read quota. Its row struct is all
/// direct, non-null columns (audited — no Nullable-decode risk).
#[cfg(test)]
mod decode_smoke {
    use super::ResolvedPoolListParams;
    use super::*;
    use crate::common::cursor::Direction;

    fn client() -> Option<clickhouse::Client> {
        let url = std::env::var("CH_URL").ok()?;
        let mut c = clickhouse::Client::default().with_url(url);
        if let Ok(u) = std::env::var("CH_USER") {
            c = c.with_user(u);
        }
        if let Ok(p) = std::env::var("CH_PASSWORD") {
            c = c.with_password(p);
        }
        if let Ok(d) = std::env::var("CH_DATABASE") {
            c = c.with_database(d);
        }
        Some(c)
    }

    /// Every LP CH row struct must decode the rows a real CH emits.
    #[tokio::test]
    async fn lp_ch_rows_decode() {
        let Some(ch) = client() else {
            eprintln!("CH_URL unset — skipping LP CH decode smoke");
            return;
        };

        // `list` returns rows on any populated CH → always exercises the
        // `PoolListChRow` decode, and bootstraps a guaranteed-real pool id for
        // the per-pool fetches below (an env-default pool might not exist on the
        // target CH → detail would return None and skip the decode entirely).
        let params = ResolvedPoolListParams {
            limit: 5,
            cursor: None,
            asset_a_code: None,
            asset_a_issuer: None,
            asset_b_code: None,
            asset_b_issuer: None,
            min_tvl: None,
            asset_code: None,
        };
        let pools = fetch_pool_list(&ch, &params, Direction::Next)
            .await
            .expect("list rows decode");

        let pool = match std::env::var("CH_TEST_POOL_HEX") {
            Ok(h) => h,
            Err(_) => match pools.first() {
                Some(r) => r.pool_id_hex.clone(),
                None => {
                    eprintln!("CH has no liquidity pools — skipping per-pool decode");
                    return;
                }
            },
        };

        // detail — `PoolDetailChRow`, incl. the Nullable-scalar `participant_count`.
        fetch_pool_by_id(&ch, &pool)
            .await
            .expect("detail row decodes");

        // participants — `ParticipantChRow`.
        fetch_participants(&ch, &pool, None, 5, Direction::Next)
            .await
            .expect("participant rows decode");

        // chart — `ChartChRow`, incl. the `samples_in_bucket` UInt64.
        let to = chrono::Utc::now();
        let from = to - chrono::Duration::days(90);
        fetch_pool_chart(&ch, &pool, "1d", from, to)
            .await
            .expect("chart rows decode");
    }
}
