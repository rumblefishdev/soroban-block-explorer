//! Small shared helpers for the backfill passes.

use clickhouse::Client as ClickhouseClient;

use crate::error::BackfillError;

/// Batch-INSERT rows into a ClickHouse table. No-op on an empty slice. Shared by
/// the backfill passes (`nft_reparse`, `balance_seed`, …) so the
/// open → write × N → end dance lives in one place.
pub(crate) async fn insert_rows<T>(
    client: &ClickhouseClient,
    table: &str,
    rows: &[T],
) -> Result<(), BackfillError>
where
    T: clickhouse::Row + clickhouse::RowOwned + serde::Serialize,
{
    if rows.is_empty() {
        return Ok(());
    }
    let mut insert = client.insert::<T>(table).await.map_err(BackfillError::Ch)?;
    for row in rows {
        insert.write(row).await.map_err(BackfillError::Ch)?;
    }
    insert.end().await.map_err(BackfillError::Ch)?;
    Ok(())
}
