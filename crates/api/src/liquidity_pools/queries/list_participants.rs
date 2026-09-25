//! `GET /v1/liquidity-pools/:id/participants` — the pool's providers.

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::resolve_accounts;
use crate::common::cursor::{Direction, keyset_sql_desc};

use crate::liquidity_pools::dto::SharesCursor;

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
    /// `100 * shares / total_pool_shares` over the pool's latest snapshot,
    /// NULL when it has none or its total is 0. Already a decimal string.
    pub share_percentage: Option<String>,
    pub first_deposit_ledger: i64,
    pub last_updated_ledger: i64,
}

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

    // `snap.ts` = total_shares of the pool's latest snapshot, however old. A
    // classic pool writes a snapshot on every change of its ledger entry, so an
    // old snapshot is a quiet pool's CURRENT state, not a stale one: a 7-day
    // window here blanked the share of 10,910 of 26,185 pools with providers
    // (production, 2026-09-25), 10,833 of which held exactly the snapshot's
    // total. The scalar subquery is scoped to the literal pool (not
    // correlated). CROSS JOIN broadcasts the single value to every position row
    // (PG `LEFT JOIN latest_snap ON TRUE`).
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
                     ORDER BY ledger_sequence DESC LIMIT 1) AS ts \
         ) snap \
         WHERE lpp.pool_id = unhex(?) AND lpp.shares > 0 \
           {keyset} \
         ORDER BY lpp.shares {order}, lpp.account_id {order} \
         LIMIT ?",
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
    // semantics preserved via filter_map.
    //
    // "A position always has its account" holds today but is NOT guaranteed by
    // construction — it is maintained by operators. Measured on prod
    // (2026-08-04): all 6010 distinct `shares > 0` participants resolved, 0
    // missing. No non-test Rust path deletes an `accounts` row and prod's
    // retained mutation history has none, but two operator-driven paths can:
    //
    //   * `docs/runbooks/0225_backfill_crash_recovery.md` rolls back `accounts`
    //     on `last_seen_ledger` while rolling back `lp_positions` on
    //     `last_updated_ledger` — DIFFERENT watermarks, so an account touched
    //     inside the crashed range can lose its row while an older position
    //     survives. That is exactly the dangling surrogate below;
    //   * `repair_tier1::rebuild_accounts` replaces the whole table via
    //     `EXCHANGE TABLES` (`ch_staging::finalize`), where rows can disappear
    //     with no DELETE at all.
    //
    // So the log stays, and the failure mode is not proportional to the cause:
    // the drop happens BEFORE `finalize_page` reads the `limit + 1` sentinel,
    // so losing the sentinel row reports "no next page" and hides the REST of
    // the list, not one participant. Only 82 of the 26_489 pools with a live
    // participant hold more than one page, but the largest holds 684. `error!`
    // (not `debug!`) because the Lambda runs at `RUST_LOG=info` (0377 F3).
    let accounts = resolve_accounts(
        client,
        rows.iter().map(|r| r.account_id_surrogate).collect(),
    )
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let Some(account) = accounts.get(&r.account_id_surrogate).cloned() else {
                tracing::error!(
                    account_id_surrogate = r.account_id_surrogate,
                    pool_id = pool_id_hex,
                    "lp_positions row resolves to no accounts row: participant \
                     dropped, so participant_count disagrees with the list and \
                     pagination may terminate early"
                );
                return None;
            };
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
