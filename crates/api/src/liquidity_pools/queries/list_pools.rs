//! `GET /v1/liquidity-pools` — the paginated pool list.

use clickhouse::Row;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

use crate::common::asset_identity::resolve_identities_and_icons;
use crate::common::ch::millis_to_utc;
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::common::pool_asset_codes::asset_codes_predicate;
use crate::common::strkey::decode_pool_kind;

use super::leg_reserves::state_reserves_sql;
use super::total_shares::{instance_shares_sql, pool_total_shares};
use super::usd_analytics::{PriceLeg, fetch_last_closes, price_leg_of, tvl_usd, usd_str};
use super::{PoolLegRow, PoolRow, fee_percent_str, leg_rows};
use crate::liquidity_pools::dto::PoolListCursor;

/// Resolved, validated `GET /v1/liquidity-pools` list params.
pub struct ResolvedPoolListParams {
    pub limit: i64,
    pub cursor: Option<PoolListCursor>,
    /// `classic` | `soroban`, already parsed. The one distinction a pool
    /// filter can make that the leg codes cannot: which protocol built it.
    pub pool_kind: Option<domain::PoolKind>,
    /// Free-text asset filter (task 0246, widened in 0440) — trimmed and
    /// uppercased at the handler boundary, then split on `/` into at most two
    /// needles. Every needle must appear on *some* leg, which makes a pair
    /// query order-insensitive without anyone knowing Stellar's canonical leg
    /// ordering. Empty = no filter.
    pub asset_codes: Vec<String>,
    /// The same free-text box, when it held a pool identifier instead
    /// (`L…` SEP-23 StrKey, the one canonical form per task 0264, resolved to
    /// the stored hex — task 0470).
    /// Mutually exclusive with `asset_codes`: an identifier names exactly one
    /// pool, so there is nothing left for a code match to narrow.
    pub pool_id_hex: Option<String>,
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

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolListChRow {
    pool_id_hex: String,
    pool_kind: i16,
    /// Leg ASSET surrogates in registration order. Resolved to identities in
    /// Rust rather than joined here: the dimensions key on `assets.id`, and the
    /// shared resolver already carries the shapes those joins have to get right.
    legs: Vec<i64>,
    fee_bps: i32,
    created_at_ledger: i64,
    /// [`ACTIVITY_LEDGER`] — the list sort/cursor key.
    cursor_ledger: i64,
    participant_count: i64,
    latest_snapshot_ledger: Option<i64>,
    reserve_a: Option<String>,
    reserve_b: Option<String>,
    total_shares: Option<String>,
    latest_snapshot_at_ms: Option<i64>,
    /// Verbatim family marker; read only by `pool_total_shares`.
    pool_type_raw: String,
    /// A soroban pool's latest reserves, raw and in leg order; empty for a
    /// classic pool, which has none there.
    state_reserves: Vec<String>,
    /// A soroban pool's raw instance-state shares, scaled in Rust (a `u128`)
    /// by its share token's decimals.
    instance_shares: Option<String>,
    share_token_id: i64,
}

/// The ordering value the list pages on: the pool's LAST ACTIVITY.
///
/// `last_updated_ledger` is the RMT version, and it means two different things.
/// A classic pool's row is rewritten on every deposit, withdrawal and trade, so
/// there it IS the last activity. A soroban pool's row is written once, at
/// registration, and never again — its activity lives in `pool_state_changes`,
/// kept per pool in `pool_activity` by a refreshable MV (schema `init.sql`).
/// Measured on production 2026-09-24: for 699 of 770 soroban pools the real
/// activity is NEWER than the column, by 250 days on average and 801 at worst,
/// and no soroban pool reached the first 5,000 rows of the list.
///
/// `greatest` rather than a per-kind branch: a classic pool has no
/// `pool_activity` row, so the join misses and the column wins — read live, it
/// never lags the MV's refresh; a soroban pool's column is its registration,
/// which its last change normally follows. A join miss yields `0`, not NULL
/// (`join_use_nulls` is refused for the read-only user), which `greatest`
/// ignores.
const ACTIVITY_LEDGER: &str = "greatest(lp.last_updated_ledger, pa.last_activity_ledger)";

/// `GET /v1/liquidity-pools` — paginated pool list. Mirrors the PG
/// `fetch_pool_list` projection, with two CH-specific structural choices
/// driven by the box-measured read cost (`liquidity_pool_snapshots` = 268 M
/// rows):
///
/// - **Order key = [`ACTIVITY_LEDGER`] (NOT `created_at_ledger`).** PG keys
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
/// - **No `min_tvl` pre-filter.** It used to exist as a `tvl_pools` CTE doing
///   a full-scan `argMax(tvl)` over the snapshot column — a column task 0199
///   established is never written, so it matched nothing. The parameter is
///   now rejected with 400 at the handler rather than silently returning an
///   empty page that contradicts the per-row `tvl` this function computes.
///   Restoring it needs TVL for ALL pools per request (it changes page
///   membership, so it cannot ride the per-page price lookup) — i.e. the
///   prices-side identity-keyed materialized series.
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

    // Keyset on `(activity_ledger, pool_id)`, expanded to scalar
    // comparisons. The cursor's `created_at_ledger` slot carries
    // the activity ledger on the CH path (opaque, ADR 0008). Bounds inlined:
    // `cursor_ledger` is i64 (no injection); `pool_id_hex` is validated hex.
    // A tampered/non-hex cursor degrades to "no keyset" (first page).
    let keyset = match params.cursor.as_ref() {
        Some(c) if is_hex_pool_id(&c.pool_id_hex) => format!(
            "AND (({act} {op} {cl}) \
                  OR ({act} = {cl} \
                      AND lower(hex(lp.pool_id)) {op} '{ph}'))",
            act = ACTIVITY_LEDGER,
            op = op,
            cl = c.created_at_ledger,
            ph = c.pool_id_hex,
        ),
        _ => String::new(),
    };

    // Asset filters are bound (untrusted free-text codes / handler-validated
    // issuer StrKeys — clickhouse-rs escapes them). Each `?` appears in the
    // `page` CTE WHERE in this exact push order. Issuer StrKey → surrogate id
    // resolves via an `accounts` PK seek (`ORDER BY (account_id)`), cheap.
    // NO relevance ranking anywhere on the pools path, by decision (task
    // 0485). Measured on production, the first page of
    // `filter[asset_codes]=XLM` is already 20 of 25 real native-leg pools —
    // they are the busiest on the network, so activity surfaces them without
    // a rule. A tier over the legs was built and taken back out: 46 lines of
    // the densest SQL in the change, for five look-alikes on page one.
    let mut binds: Vec<String> = Vec::new();
    let mut filters = String::new();
    // The per-leg POSITIONAL filters (`filter[asset_a_code]` + its issuer, and
    // the same for `b`) are gone. They named a leg by its position in a pair,
    // which a list of two-to-four legs has no equivalent for, and no caller
    // used them: the frontend's only pool filter is the free-text code box,
    // and no other client holds a key to this API. The code needle below
    // answers the same question without pinning a position.
    if let Some(kind) = params.pool_kind {
        filters.push_str(" AND lp.pool_kind = ?");
        binds.push((kind as i16).to_string());
    }
    // Asset-code needles (0440 / issue #366).
    //
    // Substring, not equality: `USD` has to match the `USDC` pools the user can
    // see on the page. `position` takes the needle literally — no LIKE wildcards
    // and no regex to escape, so caller free-text cannot widen its own match.
    // Case-insensitive here rather than `upper()` on the column: same result,
    // one pass, and it keeps working if the needle ever arrives un-normalized.
    //
    // Native legs are stored with an empty code (`asset_type = 0`, code `''`)
    // while every surface — this list included — renders them as `XLM`. Without
    // the alias, `XLM` matches none of the 11.7k pools that actually hold native
    // XLM, and instead returns ~3.7k pools of credit assets someone minted under
    // the code `XLM` (they exist, including `XLM/XLM` pairs). That is not an
    // empty result, it is a confident wrong one — so the predicate searches what
    // the row displays as.
    //
    // A pair assigns each needle its OWN leg, in either order, rather than
    // asking each needle independently whether it matches somewhere. The
    // difference only shows when the needles overlap, and then it is the whole
    // answer: `USDC/USDC` means the 72 pools with USDC on both sides, not the
    // 2 912 with USDC anywhere. Same for a needle that is a prefix of the other
    // (`USD/USDC`) — one asset must not satisfy both halves of the query.
    //
    // The predicate itself lives in `common::pool_asset_codes` because global
    // search matches pools with the SAME rule (task 0470); two copies would
    // drift, and the native arm above is exactly where a second one goes wrong.
    //
    // A pool identifier in the same box wins outright: it names one pool, so
    // it is a point seek on the primary key rather than a scan, and there is
    // nothing left for a code match to narrow. Before this, the identifier was
    // matched as a substring of an asset code and the page said "no pools".
    if let Some(pool_hex) = params.pool_id_hex.as_ref() {
        filters.push_str(" AND lp.pool_id = unhex(?)");
        binds.push(pool_hex.clone());
    } else if let Some((clause, clause_binds)) =
        asset_codes_predicate(params.asset_codes.as_slice())
    {
        filters.push_str(&format!(" AND {clause}"));
        binds.extend(clause_binds);
    }

    // Latest-snapshot fields via `argMax(...) GROUP BY pool_id` over a bounded
    // `ledger_sequence` band around the page's activity range (the `band` CTE,
    // ±10k). Page pools are the most-recently-active, so their latest snapshot
    // sits in that band — a bounded seek (~0.5M rows / ~50ms)
    // instead of a full per-pool history scan (30M rows, which OOMed the 4 GB
    // read-only profile as PR #335's `LIMIT 1 BY` sort). NO `FINAL`: the band's
    // max ledger per page pool is recent (post-0356/#318 single-image) so
    // per-column `argMax` can't tear; only a pool whose latest snapshot predates
    // #318 (inactive for weeks → deep pages) could, which is accepted.
    // The band follows the activity key, not `last_updated_ledger`: for a
    // classic pool the two are equal, while a soroban pool's column is its
    // registration — possibly years back — and would stretch the band over
    // every classic pool on the page, for a pool that has no snapshot at all.
    // `created_at_ledger` = `min(ledger_sequence)` in the `cr` subquery (cheap
    // narrow streaming scan, dup-invariant → no `FINAL`); `l_snap` seeks
    // `ledgers` by the page's ~20 `last_updated_ledger`s (a full `ledgers` join
    // built a 26M-row / 3.3 GB hash); `sac`/`asset_sac` prune to the page codes.
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
         page AS ( \
             SELECT lp.pool_id AS pool_id, lp.pool_kind AS pool_kind, \
                    lp.legs AS legs, lp.fee_bps AS fee_bps, \
                    lp.pool_type_raw AS pool_type_raw, \
                    lp.last_updated_ledger AS last_updated_ledger, \
                    {act} AS activity_ledger \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN pool_activity pa ON pa.pool_id = lp.pool_id \
             WHERE 1 = 1{filters} {keyset} \
             ORDER BY activity_ledger {order}, pool_id {order} \
             LIMIT {limit} \
         ), \
         band AS ( \
             SELECT min(activity_ledger) - 10000 AS lo, \
                    max(activity_ledger) + 10000 AS hi FROM page \
         ) \
         SELECT \
             lower(hex(lp.pool_id))                          AS pool_id_hex, \
             toInt16(lp.pool_kind)                           AS pool_kind, \
             lp.legs                                         AS legs, \
             lp.fee_bps                                      AS fee_bps, \
             ifNull(cr.created_at_ledger, lp.last_updated_ledger) AS created_at_ledger, \
             lp.activity_ledger                              AS cursor_ledger, \
             toInt64(ifNull(pc.participant_count, 0))        AS participant_count, \
             s.latest_ledger_sequence                        AS latest_snapshot_ledger, \
             toString(s.reserve_a)                           AS reserve_a, \
             toString(s.reserve_b)                           AS reserve_b, \
             toString(s.total_shares)                        AS total_shares, \
             nullIf(toUnixTimestamp64Milli(l_snap.closed_at), 0) AS latest_snapshot_at_ms, \
             lp.pool_type_raw                                AS pool_type_raw, \
             sr.reserves                                     AS state_reserves, \
             inst.shares_raw                                 AS instance_shares, \
             inst.share_token_id                             AS share_token_id \
         FROM page lp \
         LEFT JOIN ( \
             SELECT pool_id, \
                toNullable(max(ledger_sequence))                  AS latest_ledger_sequence, \
                argMax(toNullable(reserve_a), ledger_sequence)    AS reserve_a, \
                argMax(toNullable(reserve_b), ledger_sequence)    AS reserve_b, \
                argMax(toNullable(total_shares), ledger_sequence) AS total_shares \
             FROM liquidity_pool_snapshots \
             WHERE pool_id IN (SELECT pool_id FROM page) \
               AND ledger_sequence BETWEEN (SELECT lo FROM band) AND (SELECT hi FROM band) \
             GROUP BY pool_id \
         ) s ON s.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, toNullable(min(ledger_sequence)) AS created_at_ledger \
             FROM liquidity_pool_snapshots \
             WHERE pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) cr ON cr.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, count() AS participant_count FROM lp_positions FINAL \
             WHERE shares > 0 AND pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) pc ON pc.pool_id = lp.pool_id \
         LEFT JOIN ({reserves}) sr ON sr.pool_id = lp.pool_id \
         LEFT JOIN ({shares}) inst ON inst.pool_id = lp.pool_id \
         /* `GROUP BY sequence` dedups `ledgers` (ReplacingMergeTree, unmerged \
            duplicate rows): without it this LEFT JOIN doubled every page row \
            whose latest snapshot ledger falls in the duplicated range, doubling \
            UI rows and breaking keyset pagination. `any(closed_at)` is exact \
            (measured: closed_at identical across every dup pair). lore-0420 \
            \
            The filter is `page.last_updated_ledger` while the join key is \
            `s.latest_ledger_sequence` — these look mismatched but are the same \
            set, because a pool's last update always writes a snapshot at that \
            ledger: measured 52,284 of 52,284 pools with \
            `last_updated_ledger = max(ledger_sequence)`. If that invariant ever \
            breaks the join simply misses and `latest_snapshot_at_ms` is null — \
            degraded, never wrong. */ \
         LEFT JOIN ( \
             SELECT sequence, any(closed_at) AS closed_at FROM ledgers \
             WHERE sequence IN (SELECT last_updated_ledger FROM page) \
             GROUP BY sequence \
         ) l_snap ON l_snap.sequence = s.latest_ledger_sequence \
         /* The outer ORDER BY must repeat the CTE's, or the page holds the \
            right rows in the wrong order and `finalize_page` cuts the cursor \
            from the wrong last row — pages then overlap. */ \
         ORDER BY lp.activity_ledger {order}, lp.pool_id {order}",
        act = ACTIVITY_LEDGER,
        // Bounded to the page, like every other side read here.
        reserves = state_reserves_sql("SELECT pool_id FROM page"),
        shares = instance_shares_sql("SELECT pool_id FROM page"),
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

    // One batched identity resolution for every leg on the page, with the
    // icons read alongside it. Both key on `assets.id`, which is exactly what
    // `legs` stores; a soroban share token is a contract surrogate, the same
    // id space.
    // The soroban share tokens ride along: their decimals scale the shares.
    let asset_ids: BTreeSet<i64> = rows
        .iter()
        .flat_map(|r| r.legs.iter().copied().chain(Some(r.share_token_id)))
        .filter(|id| *id != 0)
        .collect();
    let (identities, icons) = resolve_identities_and_icons(client, &asset_ids).await?;

    // Phase A2 (issue #367): per-row USD TVL, computed like the detail
    // endpoint (latest reserves × last 1h close per leg; both legs required)
    // from ONE batched price lookup over the page's distinct identities.
    // `volume`/`fee_revenue` stay NULL on the list — detail-only semantics.
    // A prices error degrades every row to NULL TVL (error-logged), it does
    // not fail the list: same resilience contract as the detail endpoint.
    let page_legs: Vec<Vec<PriceLeg>> = rows
        .iter()
        .map(|r| {
            r.legs
                .iter()
                .map(|id| price_leg_of(*id, &identities))
                .collect()
        })
        .collect();
    let mut unique_legs: Vec<&PriceLeg> = page_legs
        .iter()
        .flatten()
        .filter(|l| !l.kind.is_empty())
        .collect();
    unique_legs
        .sort_unstable_by(|a, b| (a.kind, &a.code, &a.issuer).cmp(&(b.kind, &b.code, &b.issuer)));
    unique_legs.dedup();
    let closes = match fetch_last_closes(client, &unique_legs).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB error in fetch_last_closes (list TVL degraded to NULL): {e}");
            std::collections::HashMap::new()
        }
    };

    Ok(rows
        .into_iter()
        .zip(page_legs)
        .map(|(r, price_legs)| {
            // A soroban pool's reserves come from its state changes; a classic
            // pool's legs are its two snapshot columns, in order.
            let legs = leg_rows(
                &r.legs,
                &identities,
                &icons,
                &r.state_reserves,
                [r.reserve_a.as_deref(), r.reserve_b.as_deref()],
            );
            let tvl = legs_tvl(&legs, &price_legs, &closes);
            let total_shares = pool_total_shares(
                r.total_shares,
                r.instance_shares.as_deref(),
                identities.get(&r.share_token_id).and_then(|t| t.decimals),
                &r.pool_type_raw,
                &r.state_reserves,
            );
            PoolRow {
                pool_kind: decode_pool_kind(&r.pool_id_hex, r.pool_kind),
                pool_id_hex: r.pool_id_hex,
                legs,
                fee_bps: r.fee_bps,
                fee_percent: fee_percent_str(r.fee_bps),
                created_at_ledger: r.created_at_ledger,
                cursor_ledger: r.cursor_ledger,
                participant_count: r.participant_count,
                latest_snapshot_ledger: r.latest_snapshot_ledger,
                total_shares,
                tvl,
                volume: None,
                fee_revenue: None,
                latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
            }
        })
        .collect())
}

/// A pool's USD value from its legs as the page shows them. Takes the built
/// legs, never raw reserves: a soroban reserve is only in units once
/// `leg_rows` has scaled it, and a raw one would price the pool 10^7× or
/// more too high while still reading as a number.
fn legs_tvl(
    legs: &[PoolLegRow],
    price_legs: &[PriceLeg],
    closes: &HashMap<PriceLeg, f64>,
) -> Option<String> {
    let reserves: Vec<Option<&str>> = legs.iter().map(|l| l.reserve.as_deref()).collect();
    tvl_usd(&reserves, price_legs, closes).map(usd_str)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
