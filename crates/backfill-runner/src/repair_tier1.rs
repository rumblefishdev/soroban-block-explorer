//! Tier-1 column rebuild for the post-merge Hetzner CH (task 0228 Phase 5).
//!
//! ## Why this exists
//!
//! Under cross-machine parallel backfill (ADR 0040 + ADR 0045), each
//! worker ingests a disjoint ledger range into its own local CH. Worker
//! N's parser stamps `first_seen_ledger = first ledger of N's range`
//! for accounts it sees there, with no visibility into earlier worker
//! ranges. After FREEZE + rsync + ATTACH PART merges the three local
//! CHs into Hetzner, the `ReplacingMergeTree(last_seen_ledger)` collapse
//! keeps the row with the **highest** version — so the value of
//! `first_seen_ledger` ends up reflecting whatever the latest-touching
//! worker observed, not the actual earliest observation across the
//! union.
//!
//! **Not only under parallel backfill.** The same corruption happens on
//! ordinary live ingest: the indexer sees only its current batch, so any
//! later event for an entity carries no historic minimum and the RMT
//! replace erases whatever was stored. Measured 2026-09-01 —
//! `nfts.minted_at_ledger` was losing ~30 tokens/day, and 3.5% of a
//! 400-row `accounts` sample carried a `first_seen_ledger` later than the
//! true first appearance. So a clean run of this pass is point-in-time
//! cleanup, never a guarantee. Task 0497 retires the compromise itself,
//! one entry at a time as each column stops being read from its RMT copy.
//!
//! ### Scope — 2 columns × 2 tables
//!
//! | Table | Column | Correct rebuild |
//! |-------|--------|-----------------|
//! | `accounts` | `first_seen_ledger` | `MIN(ledger_sequence) FROM transaction_participants` |
//! | `lp_positions` | `first_deposit_ledger` | `MIN(ledger_sequence) FROM transaction_operations WHERE type = 22 (LiquidityPoolDeposit)` |
//!
//! **Retired entries.** `nfts.minted_at_ledger` and
//! `nfts_pending.minted_at_ledger`: both columns are dropped (task 0497) —
//! the mint is the `nft_ownership_changes` row with `event_type = 0`, which every NFT
//! read derives it from.
//! `soroban_contracts.{deployer_id, deployed_at_ledger}`: written once, from
//! the ledger change that creates the contract instance — a contract is
//! created exactly once, so no batch or worker can write a later value — and
//! carried forward unchanged by every upgrade row. The rebuild assumed the
//! deployment is the row with the smallest `wasm_uploaded_at_ledger`; once
//! that row has merged away only upgrade rows remain, and it moved the
//! deployment to the first surviving upgrade (1,652 of 154,331 contracts on
//! 2026-10-01, while the stored values agree across all rows of every
//! contract). Task 0497.
//!
//! **Source selection rule**: state-shaped tables under
//! `ReplacingMergeTree` collapse history on `OPTIMIZE FINAL`, so the
//! historic MIN must come from append-only fact tables
//! (`transaction_participants`, `transaction_operations`).
//!
//! ## Pattern
//!
//! Per-table: build `<table>_staging_repair_tier1` with the same engine
//! and schema, INSERT rebuilt rows, then `EXCHANGE TABLES` to atomically
//! swap the live table with the staging one and `DROP TABLE` the now-old
//! data. `--dry-run` short-circuits before the swap, logs the staging
//! row count, then drops the staging table — used on laptop 1's local
//! CH as a sandbox before running for real on Hetzner post-ATTACH.
//!
//! ## Idempotency
//!
//! Running twice in a row is safe. The second run re-derives the same
//! aggregates from the same source rows and produces the same staging
//! contents; the EXCHANGE then no-ops the column values. If a previous
//! run crashed mid-INSERT, the staging table is left behind — the
//! `CREATE TABLE IF NOT EXISTS` arm in [`drop_if_exists`] cleans it up
//! before the next attempt.

use clickhouse::Client as ClickhouseClient;
use tracing::info;

use crate::ch_staging::{create_staging_like, drop_if_exists, finalize, staging_row_count};
use crate::error::BackfillError;
use crate::sink::Sink;

/// Counters logged after the repair pass. Each `*_rows` is the number
/// of rows in the staging table after the rebuild INSERT — useful for
/// the operator to confirm the swap touched the expected order of
/// magnitude vs the live table's `count()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RepairTier1Stats {
    pub accounts_rows: u64,
    pub lp_positions_rows: u64,
    pub dry_run: bool,
}

/// Run the full Tier-1 column rebuild pass.
///
/// CH-only. Each table
/// rebuilds sequentially so a failure mid-way leaves the live tables
/// untouched (failures happen on staging-table writes; EXCHANGE only
/// fires after the INSERT completes).
pub async fn execute(sink: &Sink, dry_run: bool) -> Result<RepairTier1Stats, BackfillError> {
    let client = sink.client();

    let mut stats = RepairTier1Stats {
        dry_run,
        ..Default::default()
    };

    stats.accounts_rows = rebuild_accounts(client, dry_run).await?;
    stats.lp_positions_rows = rebuild_lp_positions(client, dry_run).await?;

    info!(
        accounts = stats.accounts_rows,
        lp_positions = stats.lp_positions_rows,
        dry_run,
        "repair_tier1: completed"
    );
    Ok(stats)
}

/// `accounts.first_seen_ledger` ← `MIN(tp.ledger_sequence)` joined on
/// the surrogate `accounts.id`. The transaction_participants table is
/// the participant universe — every account that has ever touched a
/// transaction shows up there, so its `min(ledger_sequence)` is the
/// authoritative earliest observation across the merged union.
///
/// COALESCE preserves the existing `first_seen_ledger` for any account
/// row that lacks a participants entry (defensive — bootstrap-only
/// accounts that came in via Soroban RPC `getLedgerEntries` but never
/// hit a tx in the indexed range; rare, but not zero).
async fn rebuild_accounts(client: &ClickhouseClient, dry_run: bool) -> Result<u64, BackfillError> {
    let staging = "accounts_staging_repair_tier1";
    drop_if_exists(client, staging).await?;
    create_staging_like(client, "accounts", staging).await?;

    let insert_sql = format!(
        "INSERT INTO {staging} (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain)
         SELECT
             a.id,
             a.account_id,
             ifNull(m.min_ledger, a.first_seen_ledger) AS first_seen_ledger,
             a.last_seen_ledger,
             a.sequence_number,
             a.home_domain
           FROM accounts AS a FINAL
           LEFT JOIN (
             SELECT account_id AS id, min(ledger_sequence) AS min_ledger
               FROM transaction_participants
              GROUP BY id
           ) AS m ON m.id = a.id"
    );
    client
        .query(&insert_sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;

    let rows = staging_row_count(client, staging).await?;
    info!(staging, rows, "repair_tier1: accounts staging built");

    finalize(client, "accounts", staging, dry_run).await?;
    Ok(rows)
}

/// `lp_positions.first_deposit_ledger` ← `MIN(ledger_sequence)` over
/// `transaction_operations` filtered to `LiquidityPoolDeposit` (op
/// type 22), joined by `(pool_id, source_id)`. The fact table
/// `transaction_operations` does not collapse under RMT in a way that
/// loses history (ORDER BY is `ledger_sequence`, `application_order`,
/// `operation_index` — distinct deposits stay distinct), so the MIN
/// there is the authoritative earliest deposit.
///
/// We deliberately do NOT read MIN from `lp_positions` itself: under
/// `RMT(last_updated_ledger)` collapse, only the latest version of the
/// `(pool, account)` row survives — MIN would then equal MAX and the
/// rebuild would no-op (or worse, overwrite a correct value with the
/// withdrawal/last-touched ledger).
///
/// `isNotNull(source_id)` + `isNotNull(pool_id)` is defensive — type
/// 22 ops should always carry both, but the schema makes both columns
/// `Nullable(…)` so an out-of-spec row would otherwise NULL the JOIN
/// key and skip a position.
async fn rebuild_lp_positions(
    client: &ClickhouseClient,
    dry_run: bool,
) -> Result<u64, BackfillError> {
    let staging = "lp_positions_staging_repair_tier1";
    drop_if_exists(client, staging).await?;
    create_staging_like(client, "lp_positions", staging).await?;

    // OperationType::LiquidityPoolDeposit = 22 (see
    // crates/domain/src/enums/operation_type.rs).
    // `lp.* REPLACE`, not a column list: the staging table is a copy of the live
    // one, and a hand-typed list silently resets every column added after it
    // was written (`closed_at_ledger`, ADR 0055) before the EXCHANGE.
    let insert_sql = format!(
        "INSERT INTO {staging}
         SELECT lp.* REPLACE (ifNull(m.min_ledger, lp.first_deposit_ledger) AS first_deposit_ledger)
           FROM lp_positions AS lp FINAL
           LEFT JOIN (
             SELECT
                 arrayJoin(pool_ids) AS pool_id,
                 source_id AS account_id,
                 min(ledger_sequence) AS min_ledger
               FROM transaction_operations
              WHERE type = 22 AND isNotNull(source_id) AND notEmpty(pool_ids)
              GROUP BY pool_id, source_id
           ) AS m ON m.pool_id = lp.pool_id AND m.account_id = lp.account_id"
    );
    client
        .query(&insert_sql)
        .execute()
        .await
        .map_err(BackfillError::Ch)?;

    let rows = staging_row_count(client, staging).await?;
    info!(staging, rows, "repair_tier1: lp_positions staging built");

    finalize(client, "lp_positions", staging, dry_run).await?;
    Ok(rows)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod columns_tests;
