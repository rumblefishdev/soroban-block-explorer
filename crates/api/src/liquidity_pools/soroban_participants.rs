//! Participants of a SOROBAN pool: the current holders of its share token
//! (task 0374).
//!
//! A classic pool's providers hold pool-share trustlines, which `lp_positions`
//! keeps ordered by pool. A soroban pool's hold its share TOKEN, so they live
//! in `balances`, ordered `(holder_id, asset_id)` — the one read in this API
//! that goes by asset. The `idx_bal_asset_id` bloom index lets it skip the
//! granules that do not hold the token: 1.16M rows / 28 MiB for a 31-holder
//! token, 17.9M / 519 MiB for the busiest, whose holders sit in 15% of the
//! granules (2026-09-22). Without the index the same query scans the table —
//! 117M rows / 1.17 GiB — and returns the same rows.

use std::collections::HashMap;

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{resolve_accounts, resolve_contracts};
use crate::common::cursor::{Direction, keyset_sql_desc};

use super::dto::SharesCursor;
use super::queries::{ParticipantRow, is_decimal_str, scale_decimal_str};

#[derive(Debug, Row, Deserialize)]
struct HolderChRow {
    holder_id: i64,
    amount: String,
    share_percentage: Option<String>,
    last_updated_ledger: i64,
}

#[derive(Debug, Row, Deserialize)]
struct DecimalsChRow {
    decimals: Option<u32>,
}

#[derive(Debug, Row, Deserialize)]
struct FirstAcquisitionChRow {
    holder: String,
    first_ledger: i64,
}

/// Holders of `share_token_id` with a positive balance, ordered
/// `(amount DESC, holder DESC)` like the classic list.
///
/// `pool_contract_id` is the pool's own contract surrogate, left out: a pair
/// locks its minimum liquidity by holding its own share token, and that is not
/// a provider — the participant count subtracts it too.
///
/// `share_percentage` divides by every positive balance of the token, the
/// pool's own holding included, so the providers' shares add up to what is
/// outstanding less the locked part. The sum is a window over the whole
/// holder set, taken before the keyset and the exclusion narrow it — a CTE
/// referenced twice would scan `balances` twice.
pub async fn fetch_soroban_participants(
    client: &clickhouse::Client,
    share_token_id: i64,
    pool_contract_id: i64,
    cursor: Option<&SharesCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<ParticipantRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);
    // Inlined, not bound, for the same reason as the classic keyset (a `None`
    // bound into a keyset returns an empty page on clickhouse-rs 0.15). A
    // soroban cursor carries the RAW integer balance, so anything else — a
    // fraction, a sign — degrades to the first page rather than reaching SQL.
    let keyset = match cursor {
        Some(c) if is_decimal_str(&c.shares) && c.shares.bytes().all(|b| b.is_ascii_digit()) => {
            format!(
                "AND ((amt {op} toInt128('{s}')) \
                      OR (amt = toInt128('{s}') AND holder_id {op} {a}))",
                s = c.shares,
                a = c.account_id,
            )
        }
        _ => String::new(),
    };
    let sql = format!(
        "SELECT holder_id, \
                toString(amt) AS amount, \
                toNullable(toString(round(amt * 100 / total, 4))) AS share_percentage, \
                lul AS last_updated_ledger \
         FROM ( \
             SELECT holder_id, amt, lul, sum(amt) OVER () AS total \
             FROM ( \
                 SELECT holder_id, \
                        argMax(amount, last_updated_ledger) AS amt, \
                        max(last_updated_ledger) AS lul \
                 FROM balances \
                 WHERE asset_id = {share_token_id} \
                 GROUP BY holder_id \
                 HAVING amt > 0 \
             ) \
         ) \
         WHERE holder_id != {pool_contract_id} {keyset} \
         ORDER BY amt {order}, holder_id {order} \
         LIMIT {limit}"
    );
    let rows = client.query(&sql).fetch_all::<HolderChRow>().await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // A holder is an account or a contract: providers routinely stake the
    // share token into a locker contract, so a contract holder is normal.
    let ids: Vec<i64> = rows.iter().map(|r| r.holder_id).collect();
    let (accounts, contracts, decimals, first_seen) = tokio::join!(
        resolve_accounts(client, ids.clone()),
        resolve_contracts(client, ids),
        fetch_token_decimals(client, share_token_id),
        fetch_first_acquisition_ledgers(client, share_token_id),
    );
    let (accounts, contracts, decimals, first_seen) =
        (accounts?, contracts?, decimals?, first_seen?);

    Ok(rows
        .into_iter()
        .filter_map(|r| {
            // Dropped loudly, with the classic path's contract: losing the
            // `limit + 1` sentinel ends the pagination early.
            let Some(holder) = accounts
                .get(&r.holder_id)
                .or_else(|| contracts.get(&r.holder_id))
                .cloned()
            else {
                tracing::error!(
                    holder_id = r.holder_id,
                    share_token_id,
                    "share-token balance resolves to no account or contract: \
                     participant dropped, pagination may terminate early"
                );
                return None;
            };
            // Every current holder is dated this way — 4,152 of 4,152 across
            // all three pool families, 2026-09-22 — because every share token
            // was deployed above the ingest floor, so the event that gave a
            // holder the token is one we hold. A miss is a defect, not a gap.
            let Some(first_deposit_ledger) = first_seen.get(&holder).copied() else {
                tracing::error!(
                    holder = %holder,
                    share_token_id,
                    "share-token holder has no mint or transfer that gave them \
                     the token: participant dropped, pagination may terminate early"
                );
                return None;
            };
            Some(ParticipantRow {
                account: holder,
                account_id_surrogate: r.holder_id,
                // Scale unknown or out of bounds → absent, never a raw
                // integer posing as a scaled amount. The percentage is
                // scale-free and stays.
                shares: decimals.and_then(|d| scale_decimal_str(&r.amount, d)),
                cursor_shares: r.amount,
                share_percentage: r.share_percentage,
                first_deposit_ledger,
                last_updated_ledger: r.last_updated_ledger,
            })
        })
        .collect())
}

/// The share token's published `decimals`, newest version. `None` when the
/// token published none.
async fn fetch_token_decimals(
    client: &clickhouse::Client,
    token_id: i64,
) -> Result<Option<u32>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT argMax(decimals, version) AS decimals \
             FROM soroban_contract_metadata \
             WHERE contract_id = ( \
                 SELECT contract_id FROM soroban_contracts WHERE id = ? LIMIT 1 \
             )",
        )
        .bind(token_id)
        .fetch_optional::<DecimalsChRow>()
        .await?;
    Ok(row.and_then(|r| r.decimals))
}

/// When each holder FIRST acquired the share token, keyed by holder StrKey.
///
/// `balances` records current state, not a first sighting, but the token's
/// own events do: a position begins at the first `mint` or incoming
/// `transfer` naming the holder. The recipient is the third element of the
/// topics for both (`JSONExtract*` indexes from 1): these share tokens'
/// `mint` carries `[sym, pool, to]`, three topics rather than the two of a
/// bare SEP-41 `mint`, so the SEP-41 index would read the POOL. That index
/// dates every current holder in all three pool families (see above).
///
/// A primary-key prefix seek: `soroban_events` is ordered on `contract_id`
/// first.
async fn fetch_first_acquisition_ledgers(
    client: &clickhouse::Client,
    token_id: i64,
) -> Result<HashMap<String, i64>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT JSONExtractString(topics_xdr, 3, 'value') AS holder, \
                    min(ledger_sequence)                      AS first_ledger \
             FROM soroban_events \
             WHERE contract_id = ? AND signature IN ('mint', 'transfer') \
             GROUP BY holder HAVING holder != ''",
        )
        .bind(token_id)
        .fetch_all::<FirstAcquisitionChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.holder, r.first_ledger))
        .collect())
}

#[cfg(test)]
mod tests;
