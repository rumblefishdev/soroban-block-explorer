//! Shared ClickHouse helpers for the transaction-list endpoints.
//!
//! ## Why this module exists
//!
//! Every endpoint that returns `TransactionListItem`-shaped rows
//! (`/transactions`, ledger detail's embedded transactions, and the
//! upcoming accounts / assets / liquidity-pool transaction lists) needs the
//! per-transaction `operation_types` + `contract_ids` arrays. The canonical
//! reference SQL computed them with **correlated scalar subqueries** in the
//! SELECT projection (`SELECT groupUniqArray(oa.type) … WHERE
//! oa.transaction_id = t.id`). ClickHouse 26.3 rejects that with
//! `Code: 48 NOT_IMPLEMENTED: can't find correlated column …` — correlated
//! subqueries referencing the outer row are unsupported.
//!
//! The fix is a **two-step, non-correlated** shape, validated against prod
//! CH 26.3:
//!
//! 1. The caller fetches the page of transactions (≤ `limit + 1` rows),
//!    yielding a bounded set of `(ledger_sequence, transaction_id)` keys.
//! 2. [`fetch_tx_list_aggregates`] aggregates `operation_types` +
//!    `contract_ids` for exactly that key set — independent derived tables
//!    keyed by `(ledger_sequence, transaction_id) IN (…)`, grouped by
//!    `transaction_id`, no reference to any outer row.
//! 3. The caller merges the aggregates back onto its page rows by
//!    `transaction_id`.
//!
//! Keys are `i64`, so they are inlined into the `IN (…)` list directly — no
//! injection surface, and it sidesteps binding a tuple array. The key set is
//! the page (≤ 101 rows), so each source scan is bounded; a partition prune
//! on `intDiv(ledger_sequence, 500000)` keeps it to the touched partition(s).

use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use clickhouse::Row;
use domain::OperationType;
use serde::Deserialize;

/// Per-transaction list aggregates, already sorted + deduplicated to match
/// the PG path's `array_agg(DISTINCT … ORDER BY …)` output.
#[derive(Debug, Default, Clone)]
pub struct TxListAggregates {
    /// Distinct operation type labels (e.g. `["INVOKE_HOST_FUNCTION", "PAYMENT"]`).
    pub operation_types: Vec<String>,
    /// Distinct C-StrKeys touched anywhere in the transaction.
    pub contract_ids: Vec<String>,
}

#[derive(Debug, Row, Deserialize)]
struct OpTypeCodesRow {
    transaction_id: i64,
    codes: Vec<i16>,
}

#[derive(Debug, Row, Deserialize)]
struct ContractIdsRow {
    transaction_id: i64,
    contract_ids: Vec<String>,
}

/// Aggregate `operation_types` + `contract_ids` for a bounded page of
/// `(ledger_sequence, transaction_id)` keys.
///
/// Returns a map keyed by `transaction_id`. A transaction with no operations
/// or no contracts is simply absent from the respective aggregate (the caller
/// treats a missing entry as the empty vec). Empty `keys` short-circuits to an
/// empty map with no query.
///
/// Non-correlated by construction (see module docs) — CH-26-safe.
pub async fn fetch_tx_list_aggregates(
    client: &clickhouse::Client,
    keys: &[(i64, i64)],
) -> Result<HashMap<i64, TxListAggregates>, clickhouse::error::Error> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }

    // `(ledger_sequence, transaction_id)` tuple-list + the touched partitions.
    // Both are `i64`, inlined directly — integers carry no injection risk.
    let in_tuples = keys
        .iter()
        .map(|(ledger, tx)| format!("({ledger},{tx})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = {
        let mut parts: Vec<i64> = keys.iter().map(|(ledger, _)| ledger / 500_000).collect();
        parts.sort_unstable();
        parts.dedup();
        parts
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };

    // Step A — operation type codes per tx (single partitioned scan).
    let op_sql = format!(
        "SELECT transaction_id, groupUniqArray(type) AS codes \
         FROM operations_appearances FINAL \
         WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
           AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
         GROUP BY transaction_id"
    );
    let op_rows = client.query(&op_sql).fetch_all::<OpTypeCodesRow>().await?;

    // Step B — contract ids per tx, unioned across the three sources a
    // contract can appear in (root op, nested invocation, event emission),
    // resolved to C-StrKeys via `soroban_contracts`.
    let ctr_sql = format!(
        "SELECT u.transaction_id AS transaction_id, \
                groupUniqArray(sc.contract_id) AS contract_ids \
         FROM ( \
            SELECT transaction_id, assumeNotNull(contract_id) AS contract_id \
            FROM operations_appearances FINAL \
            WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
              AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
              AND isNotNull(contract_id) \
            UNION ALL \
            SELECT transaction_id, contract_id \
            FROM soroban_invocations_appearances FINAL \
            WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
              AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
            UNION ALL \
            SELECT transaction_id, contract_id \
            FROM soroban_events FINAL \
            WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
              AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
         ) u \
         JOIN soroban_contracts sc FINAL ON sc.id = u.contract_id \
         GROUP BY u.transaction_id"
    );
    let ctr_rows = client.query(&ctr_sql).fetch_all::<ContractIdsRow>().await?;

    let mut map: HashMap<i64, TxListAggregates> = HashMap::with_capacity(keys.len());
    for row in op_rows {
        map.entry(row.transaction_id).or_default().operation_types =
            sorted_unique_labels(row.codes);
    }
    for row in ctr_rows {
        map.entry(row.transaction_id).or_default().contract_ids = sorted_unique(row.contract_ids);
    }
    Ok(map)
}

/// Decode a `DateTime64(3, 'UTC')` millisecond value into a `DateTime<Utc>`.
///
/// Fails loudly (`expect`) on an out-of-range value rather than degrading:
/// the input is a `ledgers.closed_at` written by the indexer from a real
/// Stellar ledger header, so an out-of-`i64`-millis-range value is a
/// data-integrity violation, not an expected condition. (Matches the 0243
/// review decision to remove the `Utc::now()` fallback in favour of
/// fail-loud — never silently substitute a wrong timestamp.)
pub fn millis_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("ClickHouse DateTime64(3, 'UTC') must decode to a valid UTC timestamp")
}

/// Map a raw `operations_appearances.type` code to its canonical
/// SCREAMING_SNAKE label, degrading to `UNKNOWN_<code>` for a code from a
/// newer protocol rather than panicking.
pub fn operation_type_label(code: i16) -> String {
    OperationType::try_from(code).map_or_else(|_| format!("UNKNOWN_{code}"), |op| op.to_string())
}

/// Map raw op-type codes to labels, then sort + dedup for deterministic PG
/// parity (`array_agg(DISTINCT … ORDER BY …)`).
pub fn sorted_unique_labels(codes: Vec<i16>) -> Vec<String> {
    sorted_unique(codes.into_iter().map(operation_type_label).collect())
}

/// Sort + dedup a string vec for deterministic PG parity.
pub fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_type_label_known_and_unknown() {
        assert_eq!(operation_type_label(0), "CREATE_ACCOUNT");
        assert_eq!(operation_type_label(1), "PAYMENT");
        assert_eq!(
            operation_type_label(i16::MAX),
            format!("UNKNOWN_{}", i16::MAX)
        );
    }

    #[test]
    fn sorted_unique_labels_dedups_and_orders() {
        assert_eq!(
            sorted_unique_labels(vec![1, 0, 1]),
            vec!["CREATE_ACCOUNT".to_string(), "PAYMENT".to_string()],
        );
    }

    #[test]
    fn sorted_unique_dedups() {
        assert_eq!(
            sorted_unique(vec!["C2".into(), "C1".into(), "C2".into()]),
            vec!["C1".to_string(), "C2".to_string()],
        );
    }

    #[test]
    fn millis_to_utc_decodes() {
        assert_eq!(
            millis_to_utc(1_700_000_000_000),
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .unwrap(),
        );
    }
}
