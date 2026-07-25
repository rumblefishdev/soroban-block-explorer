//! NFT Phase 3 post-merge reclassification on the Hetzner CH
//! (task 0118 Phase 3 + task 0217 quarantine awareness).
//!
//! ## Why this exists
//!
//! After the parallel backfill merge into Hetzner CH, the full WASM
//! universe is visible — many contracts that landed in
//! `nfts_pending` / `nft_ownership_pending` with `contract_type =
//! Other` (discriminant 1) now have a WASM-derived verdict. Three
//! outcomes need to be applied:
//!
//! 1. **Promote**: contracts that resolved to `Nft` (discriminant 2)
//!    move from `nfts_pending` → `nfts` and from
//!    `nft_ownership_pending` → `nft_ownership`.
//! 2. **Drop from quarantine**: contracts that resolved to `Fungible`
//!    (discriminant 3) — and any stray `Token` (discriminant 0,
//!    defensive; SAC tokens are dropped at persist-filter time and
//!    should never reach pending) — have their rows DELETEd from
//!    `nfts_pending` / `nft_ownership_pending`.
//! 3. **Legacy cleanup**: rows in the hot `nfts` / `nft_ownership`
//!    tables whose contract resolves to `Fungible` (3) or `Token`
//!    (0) — these are pre-quarantine false positives. Defensive
//!    DELETE; should be 0 in a fully-0217-aware pipeline but cheap
//!    to run and keeps idempotency strong.
//!
//! Contracts still classified `Other` (1) or `NULL` are left in
//! `nfts_pending` — their WASM was still not observed in the union
//! and re-classification has no input. The next backfill window
//! (Soroban era continues to extend) may surface their WASM and a
//! re-run of this pass picks them up.
//!
//! ## ContractType discriminants
//!
//! Source of truth:
//! [crates/domain/src/enums/contract_type.rs](../../../crates/domain/src/enums/contract_type.rs).
//!
//! | Discriminant | Variant    | Action |
//! |--------------|------------|--------|
//! | `0`          | `Token`    | DELETE (SAC; should not exist in pending) |
//! | `1`          | `Other`    | leave alone |
//! | `2`          | `Nft`      | promote from pending → hot |
//! | `3`          | `Fungible` | DELETE |
//!
//! ## Why FINAL on `soroban_contracts`
//!
//! `soroban_contracts` uses
//! `ReplacingMergeTree(wasm_uploaded_at_ledger)`. Non-FINAL reads can
//! return a stale `Other` (1) row even though the merged version
//! carries a later WASM-derived verdict. FINAL is required here for
//! correctness; pays the one-shot scan cost on a single-table read.
//!
//! ## Mutation semantics
//!
//! `ALTER TABLE … DELETE` is asynchronous in CH. We pass
//! `mutations_sync = 1` so each mutation blocks the client until it
//! completes — single-pass operation, no `system.mutations` polling
//! required. (Matches the existing sink test pattern.)
//!
//! `OPTIMIZE TABLE … FINAL` after mutations collapses the tombstones
//! that `ALTER TABLE … DELETE` leaves behind in `ReplacingMergeTree`
//! parts, restoring the no-FINAL-at-query-time invariant downstream.

use clickhouse::Client as ClickhouseClient;
use tracing::{debug, info};

use crate::error::BackfillError;
use crate::sink::Sink;

/// `ContractType::Token = 0` — SAC.
const CONTRACT_TYPE_TOKEN: i16 = 0;
/// `ContractType::Nft = 2`.
const CONTRACT_TYPE_NFT: i16 = 2;
/// `ContractType::Fungible = 3`.
const CONTRACT_TYPE_FUNGIBLE: i16 = 3;

#[derive(Debug, Default, Clone, Copy)]
pub struct NftReclassifyStats {
    pub promoted_nfts: u64,
    pub promoted_ownership: u64,
    pub dropped_pending_nfts: u64,
    pub dropped_pending_ownership: u64,
    pub dropped_legacy_nfts: u64,
    pub dropped_legacy_ownership: u64,
    pub dry_run: bool,
}

pub async fn execute(sink: &Sink, dry_run: bool) -> Result<NftReclassifyStats, BackfillError> {
    let client = sink.client();

    let mut stats = NftReclassifyStats {
        dry_run,
        ..Default::default()
    };

    // 1. PROMOTE pending → hot for Nft-classified contracts.
    stats.promoted_nfts = promote_or_count(client, "nfts_pending", "nfts", dry_run).await?;
    stats.promoted_ownership =
        promote_or_count(client, "nft_ownership_pending", "nft_ownership", dry_run).await?;

    // 2. DROP pending rows for Nft-promoted + Fungible/Token contracts.
    //    The Nft side completes the promotion (pending → hot, then
    //    drop from pending); Fungible/Token side is the false-positive
    //    eviction.
    let drop_discriminants = [
        CONTRACT_TYPE_TOKEN,
        CONTRACT_TYPE_NFT,
        CONTRACT_TYPE_FUNGIBLE,
    ];
    stats.dropped_pending_nfts =
        drop_or_count(client, "nfts_pending", &drop_discriminants, dry_run).await?;
    stats.dropped_pending_ownership = drop_or_count(
        client,
        "nft_ownership_pending",
        &drop_discriminants,
        dry_run,
    )
    .await?;

    // 3. LEGACY cleanup in hot tables — false positives that landed in
    //    `nfts` / `nft_ownership` from pre-0217 pipelines.
    let legacy_discriminants = [CONTRACT_TYPE_TOKEN, CONTRACT_TYPE_FUNGIBLE];
    stats.dropped_legacy_nfts =
        drop_or_count(client, "nfts", &legacy_discriminants, dry_run).await?;
    stats.dropped_legacy_ownership =
        drop_or_count(client, "nft_ownership", &legacy_discriminants, dry_run).await?;

    if !dry_run {
        // OPTIMIZE FINAL after mutations to collapse tombstones.
        // Skip per-table when this run did nothing to it — saves
        // expensive full-table scans on a re-run that's already
        // idempotent (e.g. operator runs nft-reclassify twice or
        // after Phase 5 manual rerun). Each (mutated_count) below
        // sums the promote-from-pending + the legacy-drop / pending-
        // drop work for that table.
        let touched: [(&str, u64); 4] = [
            ("nfts", stats.promoted_nfts + stats.dropped_legacy_nfts),
            ("nfts_pending", stats.dropped_pending_nfts),
            (
                "nft_ownership",
                stats.promoted_ownership + stats.dropped_legacy_ownership,
            ),
            ("nft_ownership_pending", stats.dropped_pending_ownership),
        ];
        for (tbl, mutated) in touched {
            if mutated == 0 {
                debug!(
                    table = tbl,
                    "nft_reclassify: OPTIMIZE skipped (no rows touched this run)"
                );
                continue;
            }
            client
                .query(&format!(
                    "OPTIMIZE TABLE {tbl} FINAL SETTINGS optimize_throw_if_noop = 0, max_execution_time = 3600"
                ))
                .execute()
                .await
                .map_err(BackfillError::Ch)?;
            debug!(table = tbl, mutated, "nft_reclassify: optimized");
        }
    }

    info!(
        promoted_nfts = stats.promoted_nfts,
        promoted_ownership = stats.promoted_ownership,
        dropped_pending_nfts = stats.dropped_pending_nfts,
        dropped_pending_ownership = stats.dropped_pending_ownership,
        dropped_legacy_nfts = stats.dropped_legacy_nfts,
        dropped_legacy_ownership = stats.dropped_legacy_ownership,
        dry_run,
        "nft_reclassify: completed"
    );
    Ok(stats)
}

/// Promote `pending` → `hot` for Nft-classified contracts.
///
/// Real run: `INSERT INTO hot SELECT * FROM pending WHERE contract IN
/// (Nft-classified)`. The row shapes are identical (same columns +
/// types in init.sql), so `SELECT *` is byte-safe.
///
/// Dry run: `SELECT count() FROM pending WHERE contract IN
/// (Nft-classified)` — no writes.
async fn promote_or_count(
    client: &ClickhouseClient,
    pending: &str,
    hot: &str,
    dry_run: bool,
) -> Result<u64, BackfillError> {
    let where_clause = format!(
        "contract_id IN (SELECT id FROM soroban_contracts FINAL WHERE contract_type = {CONTRACT_TYPE_NFT})"
    );
    if dry_run {
        let n: u64 = client
            .query(&format!(
                "SELECT count() FROM {pending} WHERE {where_clause}"
            ))
            .fetch_one::<u64>()
            .await
            .map_err(BackfillError::Ch)?;
        debug!(pending, hot, n, "nft_reclassify: dry-run promote count");
        return Ok(n);
    }
    // Real run. Count first so we can log a row count without re-reading.
    let n: u64 = client
        .query(&format!(
            "SELECT count() FROM {pending} WHERE {where_clause}"
        ))
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    if n == 0 {
        debug!(pending, hot, "nft_reclassify: nothing to promote");
        return Ok(0);
    }
    client
        .query(&format!(
            "INSERT INTO {hot} SELECT * FROM {pending} WHERE {where_clause}"
        ))
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    info!(pending, hot, n, "nft_reclassify: promoted rows");
    Ok(n)
}

/// Issue an `ALTER TABLE … DELETE` for rows whose contract resolves to
/// any of `discriminants`. Real run blocks on mutations completion via
/// `mutations_sync = 1`; dry run just counts.
async fn drop_or_count(
    client: &ClickhouseClient,
    table: &str,
    discriminants: &[i16],
    dry_run: bool,
) -> Result<u64, BackfillError> {
    // Build the IN-list inline. `discriminants` is a hard-coded local
    // const slice, no injection surface.
    let in_list = discriminants
        .iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let where_clause = format!(
        "contract_id IN (SELECT id FROM soroban_contracts FINAL WHERE contract_type IN ({in_list}))"
    );

    let n: u64 = client
        .query(&format!("SELECT count() FROM {table} WHERE {where_clause}"))
        .fetch_one::<u64>()
        .await
        .map_err(BackfillError::Ch)?;
    if dry_run {
        debug!(
            table,
            n,
            in_list = in_list.as_str(),
            "nft_reclassify: dry-run drop count"
        );
        return Ok(n);
    }
    if n == 0 {
        debug!(table, "nft_reclassify: nothing to drop");
        return Ok(0);
    }
    client
        .query(&format!("ALTER TABLE {table} DELETE WHERE {where_clause}"))
        .with_setting("mutations_sync", "1")
        .execute()
        .await
        .map_err(BackfillError::Ch)?;
    info!(table, n, "nft_reclassify: dropped rows");
    Ok(n)
}
