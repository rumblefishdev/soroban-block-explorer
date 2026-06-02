//! ClickHouse queries for the contracts endpoints (task 0243).
//!
//! Mirrors the PG path (`queries.rs`) — same response shapes, so the handler
//! stays backend-agnostic after the fetch. Contracts are addressed by the
//! natural C-StrKey throughout (no numeric surrogate on the wire), so there is
//! no asset-style id-mapping problem.
//!
//! `events` (canonical 14) is intentionally NOT here yet: CH `soroban_events`
//! stores `topics_xdr` / `data_xdr` inline per event, so the CH path must
//! ScVal-decode them (replacing the PG archive overlay) and adopt a 3-component
//! `(ledger_sequence, transaction_id, event_index)` keyset — a separate design
//! step. Until it lands, `API_DATASOURCE_CONTRACTS=ch` must stay off.
//!
//! Read-cost notes (lessons from the global tx-list firefight):
//! - `soroban_contracts` is ORDER BY `(contract_id)`; `soroban_invocations_appearances`
//!   is ORDER BY `(contract_id, ledger_sequence, transaction_id)`. So every
//!   filter by `contract_id` is a LEADING-primary-key seek, never a scan.
//! - A contract's invocations span MANY ledger partitions, so (like account
//!   transactions) the page is driven off the seek, then the ≤limit transaction
//!   rows are fetched by `(ledger_sequence, id) IN (keys)` and merged in Rust —
//!   never an unpruned `transactions FINAL` join (the read_rows-quota trap).

use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::millis_to_utc;
use crate::common::cursor::{Direction, direction_sql};
use crate::transactions::dto::TxListCursor;

use super::queries::{ContractRow, InterfaceRow, InvocationAppearanceRow};

/// `contract_type` SMALLINT → label, matching the PG `contract_type_name`
/// function. `None` for an out-of-range code (PG `CASE` returns NULL).
fn contract_type_name(contract_type: i16) -> Option<String> {
    match contract_type {
        0 => Some("token".to_string()),
        1 => Some("other".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Detail header — canonical 11 Statement A
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ContractHeaderChRow {
    id: i64,
    contract_id: String,
    wasm_hash: Option<String>,
    wasm_uploaded_at_ledger: Option<i64>,
    deployer: Option<String>,
    deployed_at_ledger: Option<i64>,
    contract_type: Option<i16>,
    is_sac: bool,
}

pub async fn fetch_contract(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<ContractRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                sc.id, \
                sc.contract_id, \
                lower(hex(sc.wasm_hash))               AS wasm_hash, \
                nullIf(sc.wasm_uploaded_at_ledger, 0)  AS wasm_uploaded_at_ledger, \
                nullIf(deployer.account_id, '')        AS deployer, \
                sc.deployed_at_ledger                  AS deployed_at_ledger, \
                sc.contract_type                       AS contract_type, \
                sc.is_sac                              AS is_sac \
             FROM soroban_contracts sc FINAL \
             LEFT JOIN accounts deployer ON deployer.id = sc.deployer_id \
             WHERE sc.contract_id = ? \
             LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<ContractHeaderChRow>()
        .await?;

    Ok(row.map(|r| ContractRow {
        id: r.id,
        contract_id: r.contract_id,
        wasm_hash: r.wasm_hash,
        wasm_uploaded_at_ledger: r.wasm_uploaded_at_ledger,
        deployer: r.deployer,
        deployed_at_ledger: r.deployed_at_ledger,
        contract_type_name: r.contract_type.and_then(contract_type_name),
        contract_type: r.contract_type,
        is_sac: r.is_sac,
    }))
}

// ---------------------------------------------------------------------------
// Bounded-window stats — canonical 11 Statement B
// ---------------------------------------------------------------------------

/// ~Ledgers per day at the ~5 s mainnet cadence — used to bound the
/// `(contract_id, ledger_sequence)` seek to the recent window before the exact
/// `closed_at` predicate refines it, so a hot contract's full invocation
/// history is never scanned for the 7-day stat.
const LEDGERS_PER_DAY: i64 = 17_280;

#[derive(Debug, Row, Deserialize)]
struct StatsChRow {
    recent_invocations: u64,
    recent_unique_callers: u64,
}

/// `window` is the echoed label (e.g. `"7 days"`); its leading integer is the
/// day count. CH `soroban_invocations_appearances` has no `created_at`, so the
/// window is applied via a JOIN to `ledgers.closed_at`, bounded first by a
/// `ledger_sequence` floor so the seek stays on the primary-key prefix.
pub async fn fetch_contract_stats(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    window: &str,
) -> Result<(i64, i64, String), clickhouse::error::Error> {
    let days: i64 = window
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(7);
    let ledger_floor = days.saturating_mul(LEDGERS_PER_DAY);

    // `days` / `ledger_floor` are derived from the operator-controlled window
    // label, not user input — safe to interpolate.
    let sql = format!(
        "SELECT \
            toUInt64(count())                       AS recent_invocations, \
            toUInt64(uniqExact(sia.caller_id))      AS recent_unique_callers \
         FROM soroban_invocations_appearances sia FINAL \
         INNER JOIN ledgers l ON l.sequence = sia.ledger_sequence \
         WHERE sia.contract_id = ? \
           AND sia.ledger_sequence >= (SELECT max(sequence) FROM ledgers) - {ledger_floor} \
           AND l.closed_at >= now64() - INTERVAL {days} DAY"
    );
    let row = client
        .query(&sql)
        .bind(contract_surrogate_id)
        .fetch_one::<StatsChRow>()
        .await?;

    Ok((
        row.recent_invocations as i64,
        row.recent_unique_callers as i64,
        window.to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Interface — canonical 12
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct InterfaceChRow {
    contract_id: String,
    wasm_hash: Option<String>,
    /// Raw JSON text from `wasm_interface_metadata.metadata` (empty when the
    /// contract has no WASM metadata row).
    metadata: String,
}

/// `Ok(None)` only when the contract row itself is missing; SAC / pre-upload /
/// stub contracts return `Ok(Some(_))` with `interface_metadata = None`. The
/// "has a `functions` key" stub filter (canonical 12 / PG `metadata ?
/// 'functions'`) is applied in Rust on the decoded JSON.
pub async fn fetch_wasm_interface(
    client: &clickhouse::Client,
    contract_id: &str,
) -> Result<Option<InterfaceRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                sc.contract_id, \
                lower(hex(sc.wasm_hash))        AS wasm_hash, \
                ifNull(wim.metadata, '')        AS metadata \
             FROM soroban_contracts sc FINAL \
             LEFT JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash \
             WHERE sc.contract_id = ? \
             LIMIT 1",
        )
        .bind(contract_id)
        .fetch_optional::<InterfaceChRow>()
        .await?;

    Ok(row.map(|r| {
        let interface_metadata = serde_json::from_str::<serde_json::Value>(&r.metadata)
            .ok()
            .filter(|v| v.get("functions").is_some());
        InterfaceRow {
            contract_id: r.contract_id,
            wasm_hash: r.wasm_hash,
            interface_metadata,
        }
    }))
}

// ---------------------------------------------------------------------------
// Invocations — canonical 13 (two-step, multi-partition-safe)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct InvocationKeyRow {
    ledger_sequence: i64,
    transaction_id: i64,
    caller_account: Option<String>,
    amount: i32,
}

#[derive(Debug, Row, Deserialize)]
struct TxMetaChRow {
    transaction_id: i64,
    hash: String,
    successful: bool,
    created_at: i64,
}

/// `contract_surrogate_id` is from [`fetch_contract`]; caller passes
/// `limit + 1`. Driven off `soroban_invocations_appearances` (leading-PK seek
/// on `contract_id`), then the page's transaction header columns
/// (`hash` / `successful` / `closed_at`) are fetched by
/// `(ledger_sequence, id) IN (keys)` and merged. The CH cursor keys on
/// `(ledger_sequence, transaction_id)`.
pub async fn fetch_invocation_appearances(
    client: &clickhouse::Client,
    contract_surrogate_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    direction: Direction,
) -> Result<Vec<InvocationAppearanceRow>, clickhouse::error::Error> {
    let (cursor_ledger, cursor_tiebreak): (Option<i64>, Option<i64>) = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => (Some(*ledger_sequence), Some(*tiebreak)),
        _ => (None, None),
    };
    let (op, order) = direction_sql(direction);

    // Inline the cursor bounds rather than `.bind()`-ing them: the clickhouse
    // 0.15 bound-parameter path returns an empty result when `None` is bound
    // into a tuple keyset comparison (the same defect that forced transactions
    // B/C to inline). Values are i64 / None→NULL, no injection surface.
    let cl = cursor_ledger.map_or_else(|| "NULL".to_string(), |v| v.to_string());
    let ct = cursor_tiebreak.map_or_else(|| "NULL".to_string(), |v| v.to_string());

    // Step 1: contract-scoped driver seek. `contract_id` is the leading PK of
    // `soroban_invocations_appearances`, so the inner subquery reads only this
    // contract's rows.
    //
    // The page LIMIT is applied INSIDE the subquery, BEFORE the `accounts caller`
    // join. That join has no FINAL (a 16M-row accounts FINAL would be ruinous),
    // and a hot contract has millions of invocations; joining accounts to ALL of
    // them before the limit OOMs the JoiningTransform (measured: 14.9M
    // invocations → 300M join rows → 5.6 GiB limit hit). With the limit inside,
    // the join sees only ≤limit rows. FINAL is dropped on the seek too — with it
    // CH merges the contract's rows across every part (~38× read amplification,
    // measured 574M vs 18.6M rows); the outer `LIMIT 1 BY (ledger_sequence,
    // transaction_id)` collapses both the caller-account fan-out and any rare
    // re-ingest duplicate, so FINAL is not needed for correctness here.
    let driver_sql = format!(
        "SELECT \
            m.ledger_sequence AS ledger_sequence, \
            m.transaction_id AS transaction_id, \
            nullIf(caller.account_id, '') AS caller_account, \
            m.amount AS amount \
         FROM ( \
            SELECT ledger_sequence, transaction_id, caller_id, amount \
            FROM soroban_invocations_appearances \
            WHERE contract_id = ? \
              AND ({cl} IS NULL OR (ledger_sequence, transaction_id) {op} ({cl}, {ct})) \
            ORDER BY ledger_sequence {order}, transaction_id {order} \
            LIMIT ? \
         ) m \
         LEFT JOIN accounts caller ON caller.id = m.caller_id \
         LIMIT 1 BY m.ledger_sequence, m.transaction_id"
    );
    let key_rows = client
        .query(&driver_sql)
        .bind(contract_surrogate_id)
        .bind(limit)
        .fetch_all::<InvocationKeyRow>()
        .await?;

    if key_rows.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<(i64, i64)> = key_rows
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

    // Step 2: fetch the transaction header columns for the page's keys.
    let in_tuples = keys
        .iter()
        .map(|(ledger, tx)| format!("({ledger},{tx})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = keys
        .iter()
        .map(|(ledger, _)| ledger / 500_000)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let page_sql = format!(
        "SELECT \
            t.id AS transaction_id, \
            lower(hex(t.hash)) AS hash, \
            t.successful, \
            l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions})"
    );
    let tx_rows = client.query(&page_sql).fetch_all::<TxMetaChRow>().await?;

    let mut tx_by_id: HashMap<i64, TxMetaChRow> = HashMap::with_capacity(tx_rows.len());
    for row in tx_rows {
        tx_by_id.insert(row.transaction_id, row);
    }

    // Emit in driver keyset order, merging the transaction header columns. A
    // key whose transaction row is somehow absent is skipped (should not occur
    // — an invocation appearance always has its parent transaction).
    let mut out = Vec::with_capacity(key_rows.len());
    for key in &key_rows {
        let Some(tx) = tx_by_id.get(&key.transaction_id) else {
            continue;
        };
        out.push(InvocationAppearanceRow {
            transaction_id: key.transaction_id,
            transaction_hash: tx.hash.clone(),
            ledger_sequence: key.ledger_sequence,
            created_at: millis_to_utc(tx.created_at),
            caller_account: key.caller_account.clone(),
            amount: key.amount,
            successful: tx.successful,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_type_name_matches_pg_function() {
        assert_eq!(contract_type_name(0).as_deref(), Some("token"));
        assert_eq!(contract_type_name(1).as_deref(), Some("other"));
        assert_eq!(contract_type_name(2), None);
    }
}
