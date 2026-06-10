//! Write helpers for the off-chain enrichment side tables (task 0231).
//!
//! `asset_enrichment` / `nft_enrichment` hold the SEP-1 + `token_uri`
//! enrichment columns that the indexer cannot derive from the ledger. They are
//! written ONLY by the enrichment writer (the CH-repointed `enrichment-worker`
//! Lambda, or the batch `backfill-enrichment-runner` CH mode) — never by the
//! indexer — so a concurrent indexer whole-row rewrite of `assets`/`nfts`
//! cannot clobber them (see lore task 0231,
//! `notes/R-clickhouse-enrichment-write-strategy.md`).
//!
//! `version` is the writer's processing time in **milliseconds since the Unix
//! epoch** (the wire shape of `DateTime64(3, 'UTC')`). The side tables are
//! `ReplacingMergeTree(version)`, so the highest `version` per key wins: a later
//! run can REFRESH a value, or CLEAR it by re-inserting `NULL` with a higher
//! `version`. Insert-only — no `UPDATE`, no `EXCHANGE`.

use clickhouse::Client;

use super::rows::{AssetEnrichmentRow, NftEnrichmentRow};

/// Insert a batch of SEP-1 asset enrichment rows into `asset_enrichment`.
/// No-op on an empty slice. Each row's `version` must already be set to the
/// writer's processing timestamp (ms).
pub async fn insert_asset_enrichment(
    client: &Client,
    rows: &[AssetEnrichmentRow],
) -> Result<(), clickhouse::error::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut insert = client
        .insert::<AssetEnrichmentRow>("asset_enrichment")
        .await?;
    for row in rows {
        insert.write(row).await?;
    }
    insert.end().await
}

/// Insert a batch of NFT `token_uri` enrichment rows into `nft_enrichment`.
/// No-op on an empty slice.
pub async fn insert_nft_enrichment(
    client: &Client,
    rows: &[NftEnrichmentRow],
) -> Result<(), clickhouse::error::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut insert = client.insert::<NftEnrichmentRow>("nft_enrichment").await?;
    for row in rows {
        insert.write(row).await?;
    }
    insert.end().await
}
