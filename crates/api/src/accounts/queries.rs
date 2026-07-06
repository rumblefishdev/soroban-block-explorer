//! ClickHouse queries for the accounts endpoints (task 0243).
//!
//! Returns the `AccountHeaderRow` / `AccountBalanceRow` / `AccountTxRow`
//! shapes, so the handler stays backend-agnostic after the fetch. Notable
//! CH translation choices:
//!
//! - **`asset_type_name()` is a PG SQL function.** CH has no equivalent, so
//!   the `asset_type` → label mapping is done in Rust ([`asset_type_name`]),
//!   identical to the PG migration `20260422000000_enum_label_functions`.
//! - **Surrogate ids.** `accounts.id` / `account_balances_current.account_id` /
//!   `transaction_participants.account_id` are the `Int64` cityhash surrogate;
//!   resolved from the G-StrKey via the `accounts` primary key.
//! - **Account transactions span MANY ledger partitions** (an account is
//!   active over time), so the single-partition prune used by the global
//!   `/transactions` list does NOT apply here. Instead the page is driven off
//!   `transaction_participants` (ORDER BY `(account_id, ledger_sequence,
//!   transaction_id)` → an account-scoped primary-key seek), then the ≤ `limit`
//!   transaction rows are fetched by `(ledger_sequence, id) IN (keys)`
//!   (primary-key-prefix prune per ledger, multi-partition-safe) and re-ordered
//!   in Rust. No unpruned `transactions FINAL` join — that would merge the
//!   whole 3.6B-row table (the read_rows-quota trap fixed in the global list).

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{self, millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, SortOrder, keyset_sql};
use crate::transactions::dto::TxListCursor;

use super::dto::AccountsListCursor;

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct AccountListRow {
    /// Surrogate id — cursor tiebreak only, never on the wire.
    pub id: i64,
    pub account_id: String,
    pub xlm_balance: Option<String>,
    pub last_seen_ledger: i64,
    pub first_seen_ledger: i64,
    pub home_domain: Option<String>,
}

/// Resolved, validated `GET /v1/accounts` list params handed to `fetch_list`.
pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<AccountsListCursor>,
    pub with_domain: bool,
}

#[derive(Debug)]
pub struct AccountHeaderRow {
    /// Surrogate id, threaded into balances query — never on wire.
    pub id: i64,
    pub account_id: String,
    pub first_seen_ledger: i64,
    pub last_seen_ledger: i64,
    pub sequence_number: i64,
    pub home_domain: Option<String>,
}

#[derive(Debug)]
pub struct AccountBalanceRow {
    pub asset_type_name: Option<String>,
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    pub contract_id: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub balance: String,
    pub decimals: u32,
    pub last_updated_ledger: i64,
}

#[derive(Debug)]
pub struct AccountTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub application_order: i16,
    pub source_account: String,
    pub fee_charged: i64,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// `asset_type` SMALLINT → canonical label, matching the PG `asset_type_name`
/// function (`domain::AssetType`, 4 XDR variants). `None` for an out-of-range
/// code (the PG `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native".to_string()),
        1 => Some("credit_alphanum4".to_string()),
        2 => Some("credit_alphanum12".to_string()),
        3 => Some("pool_share".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// List — GET /v1/accounts
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountListChRow {
    id: i64,
    account_id: String,
    last_seen_ledger: i64,
    first_seen_ledger: i64,
    home_domain: Option<String>,
}

/// Step-2 resolve row: `accounts.id` (surrogate) → native (`asset_type=0`) XLM
/// `balance` as text. Keyed on `account_balances_current.account_id`, which is
/// the same surrogate as `accounts.id` (task 0319).
#[derive(Debug, Row, Deserialize)]
struct AccountListBalanceRow {
    account_id: i64,
    balance: String,
}

/// CH equivalent of the PG `queries::fetch_list` (task 0274). Same response
/// shape + same opaque cursor (`AccountsListCursor{last_seen_ledger, id}` —
/// both columns exist on CH `accounts`, so the cursor is datasource-agnostic).
///
/// Divergences from PG:
///
/// - **`FINAL`** on `accounts` (ReplacingMergeTree, versioned by
///   `last_seen_ledger`) collapses re-ingested account versions to the latest;
///   the native-balance subquery is `FINAL` for the same reason.
/// - **Native balance** is the `asset_type = 0` row of
///   `account_balances_current`, mirroring the PG partial-index join. A
///   `1 AS matched` marker in the subquery + `if(abc.matched = 1, …, NULL)`
///   makes a missing native row decode as `None` (PG `NULL`). We do NOT use
///   `SETTINGS join_use_nulls = 1` — the `api_reader` CH user runs `readonly = 1`
///   (RBAC `read_only` profile), which rejects per-query setting overrides.
/// - **Cursor inlined**, not `.bind()`-ed: the clickhouse 0.15 bound-parameter
///   path returns 0 rows when `None` is bound into a tuple keyset comparison
///   (the documented defect that forced the transactions list to inline). The
///   values are `i64`, so no injection surface.
///
/// **Read-cost caveat:** the sort is `(last_seen_ledger, id)` — NOT the
/// `accounts` primary key (`account_id`) — so CH scans the whole table to
/// order it, and the `FINAL` + native-balance join widen that. Needs an
/// operator read/memory smoke on the first page before the prod flag flip,
/// same as the transactions no-filter list.
pub async fn fetch_list(
    client: &clickhouse::Client,
    params: &ResolvedListParams,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountListRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql(sort, direction);

    // Keyset clause omitted entirely on the first page (no cursor) — the unified
    // CH-list convention — so the clickhouse 0.15 "None into a tuple keyset → 0
    // rows" defect can never fire. The i64 cursor values are inlined (no
    // injection surface).
    let cursor_clause = match &params.cursor {
        Some(c) => format!(
            " AND (a.last_seen_ledger, a.id) {op} ({}, {})",
            c.last_seen_ledger, c.id
        ),
        None => String::new(),
    };
    let domain_filter = if params.with_domain {
        " AND a.home_domain IS NOT NULL"
    } else {
        ""
    };

    // Step 1: page the accounts — NO native-balance join (task 0319). The old
    // `LEFT JOIN (… account_balances_current FINAL WHERE asset_type=0)` built
    // the join side from EVERY account's native balance (~1.5M of the 2.09M
    // rows this query read), driving the ~2.2s prod TTFB. (The `accounts FINAL`
    // + non-PK `last_seen_ledger` scan+sort still costs — that needs a
    // projection, deliberately out of scope here.)
    let page_sql = format!(
        "SELECT \
            a.id                AS id, \
            a.account_id        AS account_id, \
            a.last_seen_ledger  AS last_seen_ledger, \
            a.first_seen_ledger AS first_seen_ledger, \
            a.home_domain       AS home_domain \
         FROM accounts a FINAL \
         WHERE 1{cursor_clause}{domain_filter} \
         ORDER BY a.last_seen_ledger {order}, a.id {order} \
         LIMIT ?"
    );

    let rows = client
        .query(&page_sql)
        .bind(params.limit)
        .fetch_all::<AccountListChRow>()
        .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Step 2: resolve each page account's native (XLM) balance from the unified
    // `balances` table by a PK-prefix key-seek. `balances` is ORDER BY
    // (holder_id, asset_id), so `holder_id IN (…)` seeks the prefix; `FINAL` is
    // bounded to the ≤limit page keys. The IN-list is i64 surrogates (no injection
    // surface), bounded by the page limit.
    let ids = {
        let mut v: Vec<i64> = rows.iter().map(|r| r.id).collect();
        v.sort_unstable();
        v.dedup();
        v.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    };
    let balances: HashMap<i64, String> = client
        .query(&format!(
            // task 0331 read-cutover: native (XLM) balance now from the unified
            // `balances` table (RAW Int128 = stroops). The frontend scales by
            // `decimals` (7 for native) — same raw-amount contract as the
            // account-detail balances. Resolve native via `assets.asset_type = 0`
            // (the native asset_id is a Rust cityhash, which CH `cityHash64`
            // cannot recompute, so we join rather than hardcode a literal).
            "SELECT b.holder_id AS account_id, toString(b.amount) AS balance \
             FROM balances b FINAL \
             INNER JOIN assets a FINAL ON a.id = b.asset_id \
             WHERE b.holder_id IN ({ids}) AND a.asset_type = 0"
        ))
        .fetch_all::<AccountListBalanceRow>()
        .await?
        .into_iter()
        .map(|r| (r.account_id, r.balance))
        .collect();

    Ok(rows
        .into_iter()
        .map(|r| AccountListRow {
            xlm_balance: balances.get(&r.id).cloned(),
            id: r.id,
            account_id: r.account_id,
            last_seen_ledger: r.last_seen_ledger,
            first_seen_ledger: r.first_seen_ledger,
            home_domain: r.home_domain,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Detail header — canonical 06 Statement A
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountHeaderChRow {
    id: i64,
    account_id: String,
    first_seen_ledger: i64,
    last_seen_ledger: i64,
    sequence_number: i64,
    home_domain: Option<String>,
}

/// `Ok(None)` → handler returns 404. PK seek on `accounts.account_id` + FINAL
/// (state table; latest version).
pub async fn fetch_account(
    client: &clickhouse::Client,
    account_strkey: &str,
) -> Result<Option<AccountHeaderRow>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT \
                a.id, \
                a.account_id, \
                a.first_seen_ledger, \
                a.last_seen_ledger, \
                a.sequence_number, \
                a.home_domain \
             FROM accounts a FINAL \
             WHERE a.account_id = ? \
             LIMIT 1",
        )
        .bind(account_strkey)
        .fetch_optional::<AccountHeaderChRow>()
        .await?;

    Ok(row.map(|r| AccountHeaderRow {
        id: r.id,
        account_id: r.account_id,
        first_seen_ledger: r.first_seen_ledger,
        last_seen_ledger: r.last_seen_ledger,
        sequence_number: r.sequence_number,
        home_domain: r.home_domain,
    }))
}

// ---------------------------------------------------------------------------
// Derived `deleted` status (account_merge) — task 0324
// ---------------------------------------------------------------------------

/// `true` ⟺ the account was the `source` of a **successful** `account_merge`
/// (`type = 8`) in its last-seen ledger — its ledger entry was merged into
/// another account and removed. `last_seen_ledger` is the `GREATEST` of every
/// appearance, so a deleting merge necessarily sits in that ledger and nothing
/// follows it: a later re-create would push `last_seen_ledger` higher and this
/// query would find no merge there → `false`. That makes the check a plain
/// EXISTS — no chronological "last op" ordering needed.
///
/// Two corrections over the original derivation (both were live bugs):
/// - **`successful` filter (join `transactions`).** `operations_appearances`
///   carries failed-tx ops too (no status column of its own); a *failed*
///   `account_merge` does NOT delete the account. The join restricts to
///   `t.successful`, which the single-table query could not express.
/// - **No `argMax` over `transaction_id`.** `transaction_id` is a cityhash
///   surrogate, NOT chronological, so ordering by it never picked the real last
///   op (and returned `Nullable(UInt8)`, which mismatched the `u8` decode → the
///   original 500). EXISTS sidesteps ordering and nullability entirely.
///
/// The `ledger_sequence = ?` literal on BOTH tables is load-bearing: both are
/// `PARTITION BY intDiv(ledger_sequence, 500000)` and key on `ledger_sequence`,
/// so the equality prunes each side to that one ledger (~hundreds of tx rows,
/// ~1 matching op). Without it the planner scans the 6.2B + 3.6B-row tables and
/// trips the query memory limit.
///
/// ponytail: drops the same-ledger merge-then-`create_account` re-create case
/// (merged out then recreated within the SAME ledger → still live, but EXISTS
/// reports deleted). Measured zero across 6.2B ops. To close it, anchor on the
/// real chronological key `(t.application_order, oa.application_order)` via
/// `argMax` instead of EXISTS.
pub async fn fetch_deleted_status(
    client: &clickhouse::Client,
    account_surrogate_id: i64,
    last_seen_ledger: i64,
) -> Result<bool, clickhouse::error::Error> {
    let deleted = client
        .query(
            "SELECT count() > 0 \
             FROM operations_appearances oa \
             INNER JOIN transactions t \
               ON t.id = oa.transaction_id AND t.ledger_sequence = oa.ledger_sequence \
             WHERE oa.ledger_sequence = ? \
               AND t.ledger_sequence = ? \
               AND oa.type = 8 \
               AND oa.source_id = ? \
               AND t.successful",
        )
        .bind(last_seen_ledger)
        .bind(last_seen_ledger)
        .bind(account_surrogate_id)
        .fetch_one::<u8>()
        .await?;

    Ok(deleted != 0)
}

// ---------------------------------------------------------------------------
// Detail balances — canonical 06 Statement B
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountBalanceChRow {
    asset_type: i16,
    asset_code: Option<String>,
    issuer_id: i64,
    contract_id: Option<String>,
    name: Option<String>,
    symbol: Option<String>,
    balance: String,
    decimals: u32,
    last_updated_ledger: i64,
}

/// `account_id` is the surrogate from [`fetch_account`]. Reads the unified
/// `balances` table (task 0331 Option C) by `holder_id` — a leading-PK-prefix
/// seek (`balances` ORDER BY `(holder_id, asset_id)`). Resolves each asset via the
/// `assets.id` surrogate; classic + Soroban (type-3) holdings both appear.
/// `balance` is RAW (`Int128`) — clients scale by `decimals` (classic = 7,
/// Soroban from on-chain `METADATA`).
pub async fn fetch_balances(
    client: &clickhouse::Client,
    account_id: i64,
) -> Result<Vec<AccountBalanceRow>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT \
                a.asset_type                  AS asset_type, \
                nullIf(a.asset_code, '')      AS asset_code, \
                a.issuer_id                   AS issuer_id, \
                nullIf(sc.contract_id, '')    AS contract_id, \
                coalesce(nullIf(ae.name, ''), nullIf(m.name, '')) AS name, \
                nullIf(m.symbol, '')          AS symbol, \
                toString(b.amount)            AS balance, \
                coalesce(m.decimals, 7)       AS decimals, \
                b.last_updated_ledger         AS last_updated_ledger \
             FROM balances b FINAL \
             INNER JOIN assets a FINAL ON a.id = b.asset_id \
             LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id \
             LEFT JOIN ( \
                 SELECT contract_id, name, symbol, decimals FROM soroban_contract_metadata FINAL \
             ) m ON m.contract_id = sc.contract_id \
             LEFT JOIN ( \
                 SELECT asset_type, asset_code, issuer_id, contract_id, \
                        argMax(name, version) AS name \
                 FROM asset_enrichment \
                 GROUP BY asset_type, asset_code, issuer_id, contract_id \
             ) ae ON ae.asset_type = a.asset_type AND ae.asset_code = a.asset_code \
                 AND ae.issuer_id = a.issuer_id AND ae.contract_id = a.contract_id \
             WHERE b.holder_id = ? AND b.amount != 0 \
             ORDER BY a.asset_type, a.asset_code",
        )
        .bind(account_id)
        .fetch_all::<AccountBalanceChRow>()
        .await?;

    // Resolve issuer StrKeys by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN … ON iss.id = a.issuer_id` (task 0345).
    let accounts = resolve_accounts(client, rows.iter().map(|r| r.issuer_id).collect()).await?;
    Ok(rows
        .into_iter()
        .map(|r| AccountBalanceRow {
            asset_type_name: asset_type_name(r.asset_type),
            asset_type: r.asset_type,
            asset_code: r.asset_code,
            asset_issuer: accounts
                .get(&r.issuer_id)
                .cloned()
                .filter(|s| !s.is_empty()),
            contract_id: r.contract_id,
            name: r.name,
            symbol: r.symbol,
            balance: r.balance,
            decimals: r.decimals,
            last_updated_ledger: r.last_updated_ledger,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Transactions — canonical 07 (two-step, multi-partition-safe)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ParticipantKeyRow {
    ledger_sequence: i64,
    transaction_id: i64,
}

#[derive(Debug, Row, Deserialize)]
struct AccountTxPageChRow {
    id: i64,
    hash: String,
    ledger_sequence: i64,
    application_order: i16,
    source_id: i64,
    fee_charged: i64,
    successful: bool,
    operation_count: i16,
    has_soroban: bool,
    created_at: i64,
}

/// `account_id` is the surrogate from [`fetch_account`]; caller passes
/// `limit + 1` (the peek row drives forward-continuation detection). The CH
/// cursor keys on `(ledger_sequence, transaction_id)` — a `Pg`-variant cursor
/// never reaches here (`list_account_transactions` rejects a cross-datasource
/// cursor before dispatch), so the `_` arm only ever means "first page".
pub async fn fetch_transactions(
    client: &clickhouse::Client,
    account_id: i64,
    limit: i64,
    cursor: Option<&TxListCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountTxRow>, clickhouse::error::Error> {
    let (cursor_ledger, cursor_tiebreak): (Option<i64>, Option<i64>) = match cursor {
        Some(TxListCursor::Ch {
            ledger_sequence,
            tiebreak,
        }) => (Some(*ledger_sequence), Some(*tiebreak)),
        _ => (None, None),
    };
    let (op, order) = keyset_sql(sort, direction);

    // Inline the cursor bounds rather than `.bind()`-ing them: the clickhouse
    // 0.15 bound-parameter path returns an empty result when `None` is bound
    // into a tuple keyset comparison — the same defect that forced the
    // transactions B/C statements to inline (a bound first page silently
    // returned 0 rows). Values are i64 / None→NULL, so no injection surface.
    let cl = cursor_ledger.map_or_else(|| "NULL".to_string(), |v| v.to_string());
    let ct = cursor_tiebreak.map_or_else(|| "NULL".to_string(), |v| v.to_string());

    // Step 1: account-scoped driver seek. `account_id` is the leading PK of
    // `transaction_participants`. FINAL is dropped: on a hot account it merges
    // the account's rows across every part — measured 1.16B vs 327M rows read
    // (3.5x), and one such FINAL page is ~11% of the hourly read_rows quota.
    // `LIMIT 1 BY (ledger_sequence, transaction_id)` collapses any rare
    // re-ingest duplicate, so the page still yields `limit` distinct keys
    // (box-verified: 21/21 distinct on the hottest account).
    let driver_sql = format!(
        "SELECT tp.ledger_sequence AS ledger_sequence, tp.transaction_id AS transaction_id \
         FROM transaction_participants tp \
         WHERE tp.account_id = ? \
           AND tp.ledger_sequence <= (SELECT max(sequence) FROM ledgers) \
           AND ({cl} IS NULL OR (tp.ledger_sequence, tp.transaction_id) {op} ({cl}, {ct})) \
         ORDER BY tp.ledger_sequence {order}, tp.transaction_id {order} \
         LIMIT 1 BY tp.ledger_sequence, tp.transaction_id \
         LIMIT ?"
    );
    let key_rows = client
        .query(&driver_sql)
        .bind(account_id)
        .bind(limit)
        .fetch_all::<ParticipantKeyRow>()
        .await?;

    if key_rows.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<(i64, i64)> = key_rows
        .iter()
        .map(|r| (r.ledger_sequence, r.transaction_id))
        .collect();

    // Step 2: fetch the ≤limit transaction rows by `(ledger_sequence, id) IN
    // (keys)` (primary-key-prefix prune per ledger; spans partitions safely),
    // concurrently with the operation_types aggregate for the same keys.
    // Keys are `i64`, inlined directly — no injection surface.
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
            t.id AS id, \
            lower(hex(t.hash)) AS hash, \
            t.ledger_sequence, \
            t.application_order, \
            t.source_id AS source_id, \
            t.fee_charged, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            l.closed_at AS created_at \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions})"
    );
    let (page_rows, aggregates) = tokio::join!(
        client.query(&page_sql).fetch_all::<AccountTxPageChRow>(),
        ch::fetch_tx_list_aggregates(client, &keys),
    );
    let page_rows = page_rows?;
    let aggregates = aggregates?;

    // Resolve source StrKeys by surrogate id (bloom seek) instead of a
    // whole-`accounts` `LEFT JOIN … ON src.id = t.source_id` (task 0345).
    let accounts =
        resolve_accounts(client, page_rows.iter().map(|r| r.source_id).collect()).await?;

    // Step 3: index page rows by id (a re-ingested tx collapses — values are
    // immutable), then emit in the driver's keyset order, merging
    // operation_types. `contract_ids` from the helper is intentionally unused:
    // the account-transaction item carries only `operation_types`.
    let mut by_id: HashMap<i64, AccountTxPageChRow> = HashMap::with_capacity(page_rows.len());
    for row in page_rows {
        by_id.insert(row.id, row);
    }

    let mut out = Vec::with_capacity(keys.len());
    for (_, tx_id) in &keys {
        let Some(row) = by_id.remove(tx_id) else {
            continue;
        };
        let operation_types = aggregates
            .get(tx_id)
            .map(|a| a.operation_types.clone())
            .unwrap_or_default();
        out.push(AccountTxRow {
            id: row.id,
            hash: row.hash,
            ledger_sequence: row.ledger_sequence,
            application_order: row.application_order,
            source_account: accounts
                .get(&row.source_id)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_default(),
            fee_charged: row.fee_charged,
            successful: row.successful,
            operation_count: row.operation_count,
            has_soroban: row.has_soroban,
            operation_types,
            created_at: millis_to_utc(row.created_at),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_type_name_matches_pg_function() {
        assert_eq!(asset_type_name(0).as_deref(), Some("native"));
        assert_eq!(asset_type_name(1).as_deref(), Some("credit_alphanum4"));
        assert_eq!(asset_type_name(2).as_deref(), Some("credit_alphanum12"));
        assert_eq!(asset_type_name(3).as_deref(), Some("pool_share"));
        assert_eq!(asset_type_name(99), None);
    }
}
