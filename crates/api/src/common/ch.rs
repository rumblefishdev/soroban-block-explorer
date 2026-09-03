//! Shared ClickHouse helpers for the transaction-list endpoints.
//!
//! ## Why this module exists
//!
//! Every endpoint that returns `TransactionListItem`-shaped rows
//! (`/transactions`, ledger detail's embedded transactions, and the
//! upcoming accounts / assets / liquidity-pool transaction lists) needs the
//! per-transaction `operation_types` array. The canonical reference SQL
//! computed it with a **correlated scalar subquery** in the SELECT projection
//! (`SELECT groupUniqArray(oa.type) … WHERE oa.transaction_id = t.id`).
//! ClickHouse 26.3 rejects that with `Code: 48 NOT_IMPLEMENTED: can't find
//! correlated column …` — correlated subqueries referencing the outer row are
//! unsupported.
//!
//! The fix is a **two-step, non-correlated** shape, validated against prod
//! CH 26.3:
//!
//! 1. The caller fetches the page of transactions (≤ `limit + 1` rows),
//!    yielding a bounded set of `(ledger_sequence, transaction_id)` keys.
//! 2. [`fetch_tx_list_aggregates`] aggregates `operation_types` for exactly
//!    that key set — a derived table keyed by `(ledger_sequence,
//!    transaction_id) IN (…)`, grouped by `transaction_id`, no reference to
//!    any outer row.
//! 3. The caller merges the aggregates back onto its page rows by
//!    `transaction_id`.
//!
//! Keys are `i64`, so they are inlined into the `IN (…)` list directly — no
//! injection surface, and it sidesteps binding a tuple array. The key set is
//! the page (≤ 101 rows) and a partition prune on
//! `intDiv(ledger_sequence, 500000)` confines the scan to the touched
//! partition(s).
//!
//! ## `operation_types` sources `operations_appearances` by primary-key seek
//!
//! `operations_appearances` leads its `ORDER BY` with `(ledger_sequence,
//! transaction_id)`, so the key filter is a **primary-key seek** — it reads
//! only the page transactions' op rows. `FINAL` is kept: seek-bounded here (the
//! merge is over the matched rows only — measured identical read_rows vs
//! no-FINAL), so it keeps the ReplacingMergeTree collapse explicit at zero read
//! cost, rather than leaning on `groupUniqArray` set-dedup + `type`
//! immutability.
//!
//! ## Removed: the per-row `contract_ids` array (task 0386)
//!
//! The list DTOs once carried a `contract_ids` array (every contract a tx
//! touched). It was PG-parity scaffolding — no frontend rendered it, and the
//! contract *filter* runs server-side off a UNION driver, not this array.
//! Computing it forced a whole-table `JOIN soroban_contracts FINAL` (~200k
//! rows/page, an un-pruned dimension read). The field + its aggregate were
//! deleted rather than optimised — the cheapest query is no query.

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
    /// Net-settled "value moved" per asset the transaction touched (task 0393),
    /// ordered native-first (`asset_type`, then `asset_id`) so `values[0]` is XLM
    /// when the tx moved it. Raw amounts + `decimals` — the client scales
    /// (classic/SAC = 7).
    pub values: Vec<TxValueMoved>,
}

/// One (asset, net-settled value) the transaction moved.
#[derive(Debug, Clone)]
pub struct TxValueMoved {
    /// Asset identity accepted by the asset detail endpoint's `parse_asset_id` —
    /// `"native"` or `"CODE-ISSUER"` (G-StrKey issuer). The frontend links to it.
    pub asset: String,
    /// Asset code for display (`"USDC"`); `None` for native (render as XLM).
    pub asset_code: Option<String>,
    /// Raw net-settled value (`max(Σ+, Σ−)` — the network-flow flow value);
    /// scale by `decimals`. Never NULL/0 here: the query drops both.
    pub net_settled: i128,
    /// Display decimals — `7` for classic/SAC (all assets stored in this table).
    pub decimals: u32,
}

#[derive(Debug, Row, Deserialize)]
struct OpTypeCodesRow {
    transaction_id: i64,
    codes: Vec<i16>,
}

#[derive(Debug, Row, Deserialize)]
struct TxValueChRow {
    transaction_id: i64,
    /// `toString`-encoded to avoid Int128 wire decode (matches `total_supply`).
    /// Non-nullable: `assumeNotNull` + the `IS NOT NULL` HAVING guarantee it.
    net_settled: String,
    asset_type: i16,
    asset_code: Option<String>,
    issuer_id: i64,
    /// Emitting contract C-StrKey — the identity of a bespoke Soroban token
    /// (`asset_type = 3`); `None` for classic/native.
    contract_strkey: Option<String>,
    /// Bespoke token symbol (on-chain metadata); `None` for classic/native.
    symbol: Option<String>,
    decimals: u32,
}

/// Aggregate `operation_types` and the net-settled `values` for a bounded
/// page of `(ledger_sequence, transaction_id)` keys.
///
/// Returns a map keyed by `transaction_id`. A transaction with no operations
/// is simply absent (the caller treats a missing entry as the empty vec).
/// Empty `keys` short-circuits to an empty map with no query.
///
/// Every tx-list table renders the "Net settled" column, so the value
/// aggregate always runs — no per-caller gating (the `wants_values` flag was
/// 0393 review finding F; it died with the column's 2026-09 reinstatement).
///
/// Non-correlated by construction (see module docs) — CH-26-safe.
pub async fn fetch_tx_list_aggregates(
    client: &clickhouse::Client,
    keys: &[(i64, i64)],
) -> Result<HashMap<i64, TxListAggregates>, clickhouse::error::Error> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }

    // `(ledger_sequence, transaction_id)` tuple-list + the distinct touched
    // partitions. Both are `i64`, inlined directly — integers carry no
    // injection risk.
    let in_tuples = keys
        .iter()
        .map(|(ledger, tx)| format!("({ledger},{tx})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = keys
        .iter()
        .map(|(ledger, _)| ledger / 500_000)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // Page-key filter + partition prune — the load-bearing read guard.
    // `operations_appearances` is ORDER BY `(ledger_sequence, transaction_id,
    // application_order)`, so this filter is a primary-key seek — it reads only
    // the page transactions' op rows. `FINAL` is seek-bounded here (the merge is
    // over the matched rows only, measured identical read_rows vs no-FINAL), so
    // it is kept: the RMT collapse stays explicit at zero read cost.
    let key_filter = format!(
        "(oa.ledger_sequence, oa.transaction_id) IN ({in_tuples}) \
         AND intDiv(oa.ledger_sequence, 500000) IN ({partitions})"
    );

    let op_sql = format!(
        "SELECT oa.transaction_id AS transaction_id, \
                groupUniqArray(oa.type) AS codes \
         FROM operations_appearances oa FINAL \
         WHERE {key_filter} \
         GROUP BY oa.transaction_id"
    );

    // Second aggregate: net-settled "value moved" per (tx, asset) (task 0393).
    // `max(net_settled)` is the read-side dedup over the version-less RMT: the
    // live indexer and the 0383 backfill reduce the SAME value from the SAME
    // events, so duplicate rows agree, and `max` ignores NULL — so a computed
    // value wins over a "not computed" row for the same key. (There is no
    // engine version: a downward correction only follows a change to our own
    // reducer, handled by a re-backfill + OPTIMIZE, not per-row versioning.)
    //
    // `net_settled` is `Nullable(Int128)`: NULL = not computable, 0 = genuinely
    // nothing settled. HAVING drops both — uninteresting to show — but they stay
    // distinguishable at rest, the point of the nullable column. `assumeNotNull`
    // is safe only because HAVING has already excluded NULL; without it the
    // aggregate over a Nullable column decodes as Nullable(String) into a
    // non-nullable field and 500s (the trap that took account-detail down, 0324).
    //
    // NOTE (0393 read-path, deferred to the task's Operations section): the table
    // is `asset_id`-leading, so this `(ledger, tx)` filter SCANS the pruned
    // partition (~26M rows/page measured, vs ~16k for the op-types seek beside
    // it), and the three dimension joins below are un-pruned (`assets.id` is not
    // in its ORDER BY). A `(ledger, tx)` companion (accounts_recent pattern) is
    // required before this ships at scale. `decimals` = 7 for classic/SAC; the
    // metadata coalesce keeps bespoke type-3 correct. Ordered `asset_type` first
    // so native (type 0 = XLM) sorts ahead of credit — the frontend takes the
    // first row as the primary asset (0393 decision: native-first, not hash order).
    let value_sql = format!(
        "SELECT oaa.transaction_id             AS transaction_id, \
                toString(assumeNotNull(max(oaa.net_settled))) AS net_settled, \
                any(a.asset_type)              AS asset_type, \
                any(nullIf(a.asset_code, ''))  AS asset_code, \
                any(a.issuer_id)               AS issuer_id, \
                any(nullIf(sc.contract_id, '')) AS contract_strkey, \
                any(nullIf(m.symbol, ''))      AS symbol, \
                any(coalesce(m.decimals, 7))   AS decimals \
         FROM operation_asset_appearances oaa \
         INNER JOIN assets a FINAL ON a.id = oaa.asset_id \
         LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id \
         LEFT JOIN ( \
             SELECT contract_id, symbol, decimals FROM soroban_contract_metadata FINAL \
         ) m ON m.contract_id = sc.contract_id \
         WHERE (oaa.ledger_sequence, oaa.transaction_id) IN ({in_tuples}) \
           AND intDiv(oaa.ledger_sequence, 500000) IN ({partitions}) \
         GROUP BY oaa.transaction_id, oaa.asset_id \
         HAVING max(oaa.net_settled) IS NOT NULL \
            AND max(oaa.net_settled) != 0 \
         ORDER BY oaa.transaction_id, asset_type, oaa.asset_id"
    );
    // Both aggregates are independent non-correlated queries over the same page
    // keys — run them concurrently to overlap the round-trips. `resolve_accounts`
    // below depends on `value_rows`, so it stays sequential after.
    let (op_rows, value_rows) = tokio::try_join!(
        client.query(&op_sql).fetch_all::<OpTypeCodesRow>(),
        client.query(&value_sql).fetch_all::<TxValueChRow>(),
    )?;

    let mut map: HashMap<i64, TxListAggregates> = HashMap::with_capacity(keys.len());
    for row in op_rows {
        map.entry(row.transaction_id).or_default().operation_types =
            sorted_unique_labels(row.codes);
    }
    // Resolve issuer surrogates -> G-StrKeys (bloom-pruned key seek) so a credit
    // asset's `parse_asset_id` link identity `CODE-ISSUER` can be built.
    let issuer_ids: Vec<i64> = value_rows
        .iter()
        .filter(|r| r.asset_type != 0 && r.issuer_id != 0)
        .map(|r| r.issuer_id)
        .collect();
    let issuers = resolve_accounts(client, issuer_ids).await?;
    for row in value_rows {
        let Ok(net_settled) = row.net_settled.parse::<i128>() else {
            continue;
        };
        // Asset identity for the `parse_asset_id` link + display code, per
        // `TokenAssetType`: 0 native, 1 classic credit, 3 bespoke Soroban.
        let (asset, asset_code) = match row.asset_type {
            0 => ("native".to_string(), None),
            3 => {
                // Bespoke Soroban token: identity is its contract C-StrKey
                // (`parse_asset_id` accepts a `C…`); display code is its symbol.
                (
                    row.contract_strkey.clone().unwrap_or_default(),
                    row.symbol.clone(),
                )
            }
            _ => {
                // Classic credit — `CODE-ISSUER` (G-StrKey issuer).
                let code = row.asset_code.clone().unwrap_or_default();
                let issuer = issuers.get(&row.issuer_id).cloned().unwrap_or_default();
                (format!("{code}-{issuer}"), row.asset_code)
            }
        };
        map.entry(row.transaction_id)
            .or_default()
            .values
            .push(TxValueMoved {
                asset,
                asset_code,
                net_settled,
                decimals: row.decimals,
            });
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

// ---------------------------------------------------------------------------
// Surrogate `id` -> StrKey resolution (tasks 0344 / 0345)
//
// Replaces the whole-dimension `JOIN accounts/soroban_contracts (…) ON x.id =
// <surrogate>` anti-pattern: that join reads the ENTIRE dimension table to build
// its hash side (surrogate `id` is not the sort key), even to resolve a handful
// of ids — the ~25M-row cost seen across the detail/entity endpoints. Instead we
// fetch the surrogate ids off the driver rows and look StrKeys up by `id`:
// `accounts`/`soroban_contracts` have a bloom skip index on `id`, so
// `WHERE id IN (…)` is a granule seek. `LIMIT 1 BY id` picks any
// ReplacingMergeTree version — exact because the StrKey (`account_id` /
// `contract_id`) is immutable across versions (proven byte-identical in 0344).
//
// The returned map is RAW (may contain empty StrKeys). Callers apply their own
// `nullIf('')` — `map.get(&id).filter(|s| !s.is_empty())` matches the old
// `LEFT JOIN … nullIf(x, '')`; a plain `map.get(&id)` matches an `INNER JOIN`.
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct IdStrKeyRow {
    id: i64,
    strkey: String,
}

async fn resolve_id_strkey(
    client: &clickhouse::Client,
    table: &str,
    strkey_col: &str,
    mut ids: Vec<i64>,
) -> Result<HashMap<i64, String>, clickhouse::error::Error> {
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let in_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    Ok(client
        .query(&format!(
            "SELECT id, {strkey_col} AS strkey FROM {table} WHERE id IN ({in_list}) LIMIT 1 BY id"
        ))
        .fetch_all::<IdStrKeyRow>()
        .await?
        .into_iter()
        .map(|r| (r.id, r.strkey))
        .collect())
}

/// `accounts.id` -> `account_id` (StrKey). See the module note above.
pub async fn resolve_accounts(
    client: &clickhouse::Client,
    ids: Vec<i64>,
) -> Result<HashMap<i64, String>, clickhouse::error::Error> {
    resolve_id_strkey(client, "accounts", "account_id", ids).await
}

/// `soroban_contracts.id` -> `contract_id` (StrKey). See the module note above.
pub async fn resolve_contracts(
    client: &clickhouse::Client,
    ids: Vec<i64>,
) -> Result<HashMap<i64, String>, clickhouse::error::Error> {
    resolve_id_strkey(client, "soroban_contracts", "contract_id", ids).await
}

/// Build a ClickHouse client from `CH_URL` (+ optional `CH_USER` /
/// `CH_PASSWORD` / `CH_DATABASE`) for the DB-backed handler tests. Returns
/// `None` when `CH_URL` is unset so the tests skip cleanly and CI (no CH
/// access) stays green — shared by every `#[cfg(test)]` module that needs a
/// live client.
#[cfg(test)]
pub(crate) fn test_client_from_env() -> Option<clickhouse::Client> {
    let url = std::env::var("CH_URL").ok()?;
    let mut c = clickhouse::Client::default().with_url(url);
    if let Ok(u) = std::env::var("CH_USER") {
        c = c.with_user(u);
    }
    if let Ok(p) = std::env::var("CH_PASSWORD") {
        c = c.with_password(p);
    }
    if let Ok(d) = std::env::var("CH_DATABASE") {
        c = c.with_database(d);
    }
    Some(c)
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
