//! Transaction hash → ledger, the first step of the transaction page.

use clickhouse::Row;
use serde::Deserialize;

#[derive(Debug, Row, Deserialize)]
struct LedgerSeqRow {
    ledger_sequence: i64,
}

/// The ledgers a transaction hash — outer, or a fee-bump's inner — may live
/// in, newest first.
///
/// `transaction_hash_prefix_index` is keyed by the hash's first 8 bytes (task
/// 0580), so a seek returns more than one ledger when two hashes share the
/// prefix; the caller keeps the one whose `transactions` row carries the full
/// hash. Nearly always one ledger, or none. `hash → ledger_sequence` is
/// immutable, so no `FINAL`; `DISTINCT` folds a re-ingested duplicate.
pub async fn lookup_hash_ledgers(
    client: &clickhouse::Client,
    hash_hex: &str,
) -> Result<Vec<i64>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT DISTINCT ledger_sequence FROM transaction_hash_prefix_index \
             WHERE hash_prefix = reinterpretAsUInt64(substring(unhex(?), 1, 8)) \
             ORDER BY ledger_sequence DESC",
        )
        .bind(hash_hex)
        .fetch_all::<LedgerSeqRow>()
        .await?;
    Ok(rows.into_iter().map(|r| r.ledger_sequence).collect())
}
