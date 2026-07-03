//! Mirrors canonical SQL `endpoint-queries/{06,07}_*.sql`.
//! `transaction_participants` includes source, so no UNION with `source_id`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::common::cursor::{Direction, SortOrder, TsIdCursor, keyset_sql};

// ---------------------------------------------------------------------------
// GET /v1/accounts (list)
// ---------------------------------------------------------------------------

/// Keyset cursor for the accounts list. Sort is `last_seen_ledger` with the
/// surrogate `id` as the unique tiebreak (`last_seen_ledger` is not unique).
/// Mirrors lp's `SharesCursor` (value + id) but uses `keyset_sql` so the
/// base `?order=` can flip, like the ledgers list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsListCursor {
    pub last_seen_ledger: i64,
    pub id: i64,
}

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

pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<AccountsListCursor>,
    pub with_domain: bool,
}

/// `xlm_balance` is the native row from `account_balances_current` (served by
/// the partial unique index `uidx_abc_native (account_id) WHERE asset_type=0`);
/// `None` when the account has no native balance row.
const ACCOUNT_LIST_SELECT: &str = "SELECT a.id, \
     a.account_id, \
     a.last_seen_ledger, \
     a.first_seen_ledger, \
     a.home_domain, \
     abc.balance::text AS xlm_balance \
     FROM accounts a \
     LEFT JOIN account_balances_current abc \
       ON abc.account_id = a.id AND abc.asset_type = 0";

fn push_glue(qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, has_where: &mut bool) {
    qb.push(if *has_where { " AND" } else { " WHERE" });
    *has_where = true;
}

fn map_list_row(r: &PgRow) -> AccountListRow {
    AccountListRow {
        id: r.get("id"),
        account_id: r.get("account_id"),
        xlm_balance: r.get("xlm_balance"),
        last_seen_ledger: r.get("last_seen_ledger"),
        first_seen_ledger: r.get("first_seen_ledger"),
        home_domain: r.get("home_domain"),
    }
}

pub async fn fetch_list(
    pool: &PgPool,
    params: &ResolvedListParams,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountListRow>, sqlx::Error> {
    let (op, order) = keyset_sql(sort, direction);

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(ACCOUNT_LIST_SELECT);
    let mut has_where = false;

    if params.with_domain {
        push_glue(&mut qb, &mut has_where);
        qb.push(" a.home_domain IS NOT NULL");
    }
    if let Some(cursor) = &params.cursor {
        push_glue(&mut qb, &mut has_where);
        qb.push(format!(" (a.last_seen_ledger, a.id) {op} ("));
        qb.push_bind(cursor.last_seen_ledger);
        qb.push(", ");
        qb.push_bind(cursor.id);
        qb.push(")");
    }

    qb.push(format!(
        " ORDER BY a.last_seen_ledger {order}, a.id {order} LIMIT "
    ));
    qb.push_bind(params.limit);

    let raw: Vec<PgRow> = qb.build().fetch_all(pool).await?;
    Ok(raw.iter().map(map_list_row).collect())
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

// ---------------------------------------------------------------------------
// Detail header query — canonical 06 Statement A
// ---------------------------------------------------------------------------

/// `Ok(None)` → handler returns 404.
pub async fn fetch_account(
    pool: &PgPool,
    account_strkey: &str,
) -> Result<Option<AccountHeaderRow>, sqlx::Error> {
    let raw: Option<PgRow> = sqlx::query(
        "SELECT \
            a.id, \
            a.account_id, \
            a.first_seen_ledger, \
            a.last_seen_ledger, \
            a.sequence_number, \
            a.home_domain \
         FROM accounts a \
         WHERE a.account_id = $1",
    )
    .bind(account_strkey)
    .fetch_optional(pool)
    .await?;

    Ok(raw.map(|r| AccountHeaderRow {
        id: r.get("id"),
        account_id: r.get("account_id"),
        first_seen_ledger: r.get("first_seen_ledger"),
        last_seen_ledger: r.get("last_seen_ledger"),
        sequence_number: r.get("sequence_number"),
        home_domain: r.get("home_domain"),
    }))
}

// Detail balances are ClickHouse-only (unified `balances` model, task 0331); the
// legacy PG `fetch_balances` over `account_balances_current` was cut (PG retired).

// ---------------------------------------------------------------------------
// Transactions query — canonical 07
// ---------------------------------------------------------------------------

/// Caller threads the surrogate `account_id` (from [`fetch_account`]) so the
/// planner walks the `transaction_participants` PK keyset directly. Caller
/// passes `limit + 1`.
pub async fn fetch_transactions(
    pool: &PgPool,
    account_id: i64,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    sort: SortOrder,
    direction: Direction,
) -> Result<Vec<AccountTxRow>, sqlx::Error> {
    let cursor_ts = cursor.map(|c| c.ts);
    let cursor_id = cursor.map(|c| c.id);
    let (op, order) = keyset_sql(sort, direction);

    let sql = format!(
        "SELECT \
             t.id, \
             encode(t.hash, 'hex')          AS hash, \
             t.ledger_sequence, \
             t.application_order, \
             src.account_id                 AS source_account, \
             t.fee_charged, \
             t.successful, \
             t.operation_count, \
             t.has_soroban, \
             COALESCE(ops.operation_types, ARRAY[]::text[]) AS operation_types, \
             t.created_at \
         FROM transaction_participants tp \
         JOIN transactions t \
                ON t.id         = tp.transaction_id \
               AND t.created_at = tp.created_at \
         JOIN accounts src ON src.id = t.source_id \
         LEFT JOIN LATERAL ( \
             SELECT array_agg(DISTINCT op_type_name(oa.type) \
                              ORDER BY op_type_name(oa.type)) AS operation_types \
             FROM operations_appearances oa \
             WHERE oa.transaction_id = t.id \
               AND oa.created_at     = t.created_at \
         ) ops ON TRUE \
         WHERE tp.account_id = $1 \
           AND ($3::timestamptz IS NULL OR (tp.created_at, tp.transaction_id) {op} ($3, $4)) \
         ORDER BY tp.created_at {order}, tp.transaction_id {order} \
         LIMIT $2"
    );

    let raw: Vec<PgRow> = sqlx::query(&sql)
        .bind(account_id)
        .bind(limit)
        .bind(cursor_ts)
        .bind(cursor_id)
        .fetch_all(pool)
        .await?;

    Ok(raw
        .iter()
        .map(|r| AccountTxRow {
            id: r.get("id"),
            hash: r.get("hash"),
            ledger_sequence: r.get("ledger_sequence"),
            application_order: r.get("application_order"),
            source_account: r.get("source_account"),
            fee_charged: r.get("fee_charged"),
            successful: r.get("successful"),
            operation_count: r.get("operation_count"),
            has_soroban: r.get("has_soroban"),
            operation_types: r.get("operation_types"),
            created_at: r.get("created_at"),
        })
        .collect())
}
