//! Transaction hash → ledger, the first step of the transaction page.

use clickhouse::Row;
use serde::Deserialize;

#[derive(Debug, Row, Deserialize)]
struct LedgerSeqRow {
    ledger_sequence: i64,
}

/// Resolve a transaction hash → parent `ledger_sequence`.
///
/// Reads `transaction_hash_index` directly (PK seek on `hash`), mirroring
/// the PG `lookup_hash_index`. `hash → ledger_sequence` is immutable, so no
/// `FINAL` is required on the ReplacingMergeTree index.
pub async fn lookup_hash_ledger(
    client: &clickhouse::Client,
    hash_hex: &str,
) -> Result<Option<i64>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT ledger_sequence FROM transaction_hash_index \
             WHERE hash = unhex(?) LIMIT 1",
        )
        .bind(hash_hex)
        .fetch_optional::<LedgerSeqRow>()
        .await?;
    Ok(row.map(|r| r.ledger_sequence))
}
