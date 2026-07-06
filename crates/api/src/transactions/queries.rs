//! Database queries for the transactions endpoints.

use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::common::cursor::{Direction, keyset_sql_desc};

use super::dto::TxListCursor;

#[derive(Debug)]
pub struct TxListRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub application_order: i16,
    /// `None` for Variant A `parse_error` transactions whose envelope was
    /// unavailable (lore-0209). The accompanying `parse_error: true` flag
    /// signals to consumers that other fields may be incomplete too.
    pub source_account: Option<String>,
    pub fee_charged: i64,
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
    pub contract_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct TxDetailRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub application_order: i16,
    /// `None` for Variant A `parse_error` transactions whose envelope was
    /// unavailable (lore-0209).
    pub source_account: Option<String>,
    pub fee_charged: i64,
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub created_at: DateTime<Utc>,
    pub parse_error: bool,
}

/// Result of the `transaction_hash_index` seek: the `created_at` partition
/// key the detail read needs to key the `transactions` row by
/// `(hash, created_at)`. `ledger_sequence` is not carried here — the detail
/// handler reads it back from the resolved `TxDetailRow` (same value, fewer
/// fields to keep in sync).
#[derive(Debug)]
pub struct HashIndexRow {
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct OpRow {
    pub appearance_id: i64,
    pub type_name: String,
    pub op_type: i16,
    pub source_account: Option<String>,
    pub destination_account: Option<String>,
    pub contract_id: Option<String>,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// Crossed liquidity pools, hex-encoded. PG stores the legacy scalar
    /// (0 or 1 element after mapping); CH stores the full crossed-pool
    /// list from path-payment claim atoms (task 0261/0268).
    pub pool_ids: Vec<String>,
    /// 1-based per-tx apply position (task 0192). `None` for pre-task-0192
    /// rows where the column was not yet populated by the indexer; the
    /// caller falls back to `appearance_id` ordering for those.
    pub application_order: Option<i16>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct EventAppearanceRow {
    pub contract_id: String,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct InvocationAppearanceRow {
    pub contract_id: String,
    pub caller_account: Option<String>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<TxListCursor>,
    pub source_account: Option<String>,
    pub contract_id: Option<String>,
    pub op_type: Option<i16>,
}

fn push_glue(qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, has_where: &mut bool) {
    qb.push(if *has_where { " AND" } else { " WHERE" });
    *has_where = true;
}

fn push_cursor_predicate(
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    ts: DateTime<Utc>,
    id: i64,
    op: &str,
) {
    qb.push(" (t.created_at, t.id) ");
    qb.push(op);
    qb.push(" (");
    qb.push_bind(ts);
    qb.push(", ");
    qb.push_bind(id);
    qb.push(")");
}

fn map_list_row(r: &PgRow) -> TxListRow {
    TxListRow {
        id: r.get("id"),
        hash: r.get("hash"),
        ledger_sequence: r.get("ledger_sequence"),
        application_order: r.get("application_order"),
        source_account: r.get("source_account"),
        fee_charged: r.get("fee_charged"),
        inner_tx_hash: r.get("inner_tx_hash"),
        successful: r.get("successful"),
        operation_count: r.get("operation_count"),
        has_soroban: r.get("has_soroban"),
        operation_types: r.get("operation_types"),
        contract_ids: r.get("contract_ids"),
        created_at: r.get("created_at"),
    }
}

const LIST_PROJECTION: &str = "\
    SELECT \
        t.id, \
        encode(t.hash, 'hex')          AS hash, \
        t.ledger_sequence, \
        t.application_order, \
        src.account_id                 AS source_account, \
        t.fee_charged, \
        encode(t.inner_tx_hash, 'hex') AS inner_tx_hash, \
        t.successful, \
        t.operation_count, \
        t.has_soroban, \
        COALESCE(ops.operation_types, ARRAY[]::text[]) AS operation_types, \
        COALESCE(ctr.contract_ids,    ARRAY[]::text[]) AS contract_ids, \
        t.created_at\
    ";

const LIST_LATERAL_BLOCKS: &str = "\
    LEFT JOIN LATERAL ( \
        SELECT array_agg(DISTINCT op_type_name(oa.type) ORDER BY op_type_name(oa.type)) AS operation_types \
        FROM operations_appearances oa \
        WHERE oa.transaction_id = t.id \
          AND oa.created_at     = t.created_at \
    ) ops ON TRUE \
    LEFT JOIN LATERAL ( \
        SELECT array_agg(DISTINCT sc.contract_id ORDER BY sc.contract_id) AS contract_ids \
        FROM ( \
            SELECT contract_id FROM operations_appearances \
            WHERE transaction_id = t.id \
              AND created_at     = t.created_at \
              AND contract_id IS NOT NULL \
            UNION \
            SELECT contract_id FROM soroban_invocations_appearances \
            WHERE transaction_id = t.id \
              AND created_at     = t.created_at \
            UNION \
            SELECT contract_id FROM soroban_events_appearances \
            WHERE transaction_id = t.id \
              AND created_at     = t.created_at \
        ) all_ctr \
        JOIN soroban_contracts sc ON sc.id = all_ctr.contract_id \
    ) ctr ON TRUE\
    ";

pub async fn fetch_list(
    pool: &PgPool,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<TxListRow>, sqlx::Error> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
    let (op, order) = keyset_sql_desc(direction);

    // PG keys on `(created_at, id)`. Extract the `Pg` cursor variant; a
    // non-`Pg` cursor never reaches here (`list_transactions` rejects a
    // cross-datasource cursor with `invalid_cursor` before dispatch), so the
    // `_` arm only ever means "first page".
    let cursor: Option<(DateTime<Utc>, i64)> = match &params.cursor {
        Some(TxListCursor::Pg { ts, id }) => Some((*ts, *id)),
        _ => None,
    };

    match (&params.contract_id, params.op_type) {
        (Some(cid), op_type_opt) => {
            qb.push("WITH matched_tx AS (SELECT DISTINCT created_at, transaction_id FROM (");
            push_contract_union_arm(&mut qb, "operations_appearances", cid, cursor, op);
            qb.push(" UNION ");
            push_contract_union_arm(&mut qb, "soroban_invocations_appearances", cid, cursor, op);
            qb.push(" UNION ");
            push_contract_union_arm(&mut qb, "soroban_events_appearances", cid, cursor, op);

            qb.push(format!(
                ") u ORDER BY created_at {order}, transaction_id {order} LIMIT "
            ));
            // 4× over-fetch — a single tx may appear in up to all three arms.
            qb.push_bind(params.limit * 4);
            qb.push(") ");

            qb.push(LIST_PROJECTION);
            qb.push(
                " FROM matched_tx m \
                 JOIN transactions t ON t.id = m.transaction_id AND t.created_at = m.created_at \
                 JOIN accounts src ON src.id = t.source_id ",
            );
            qb.push(LIST_LATERAL_BLOCKS);

            let mut has_where = false;
            if let Some(acct) = &params.source_account {
                push_glue(&mut qb, &mut has_where);
                push_source_predicate(&mut qb, acct);
            }
            if let Some(op_type) = op_type_opt {
                push_glue(&mut qb, &mut has_where);
                qb.push(
                    " EXISTS (SELECT 1 FROM operations_appearances oa2 \
                       WHERE oa2.transaction_id = t.id \
                         AND oa2.created_at     = t.created_at \
                         AND oa2.type           = ",
                );
                qb.push_bind(op_type);
                qb.push(")");
            }
        }

        (None, Some(op_type)) => {
            qb.push(
                "WITH matched_ops AS (\
                   SELECT DISTINCT ON (oa.created_at, oa.transaction_id) \
                          oa.transaction_id, oa.created_at \
                   FROM operations_appearances oa \
                   WHERE oa.type = ",
            );
            qb.push_bind(op_type);
            if let Some((ts, id)) = cursor {
                qb.push(format!(" AND (oa.created_at, oa.transaction_id) {op} ("));
                qb.push_bind(ts);
                qb.push(", ");
                qb.push_bind(id);
                qb.push(")");
            }
            qb.push(format!(
                " ORDER BY oa.created_at {order}, oa.transaction_id {order}, oa.id LIMIT "
            ));
            qb.push_bind(params.limit * 4);
            qb.push(") ");

            qb.push(LIST_PROJECTION);
            qb.push(
                " FROM matched_ops m \
                 JOIN transactions t ON t.id = m.transaction_id AND t.created_at = m.created_at \
                 JOIN accounts src ON src.id = t.source_id ",
            );
            qb.push(LIST_LATERAL_BLOCKS);

            let mut has_where = false;
            if let Some(acct) = &params.source_account {
                push_glue(&mut qb, &mut has_where);
                push_source_predicate(&mut qb, acct);
            }
            // Re-applied: CTE LIMIT $1*4 may overshoot the cursor boundary.
            if let Some((ts, id)) = cursor {
                push_glue(&mut qb, &mut has_where);
                push_cursor_predicate(&mut qb, ts, id, op);
            }
        }

        (None, None) => {
            qb.push(LIST_PROJECTION);
            // lore-0209: LEFT JOIN so Variant A `parse_error` transactions
            // (NULL `source_id`) surface in the unfiltered list. Filtered
            // branches above keep the inner JOIN because their CTE drivers
            // (operations / events / invocations) never match parse_error
            // rows in the first place.
            qb.push(" FROM transactions t LEFT JOIN accounts src ON src.id = t.source_id ");
            qb.push(LIST_LATERAL_BLOCKS);

            let mut has_where = false;
            if let Some(acct) = &params.source_account {
                push_glue(&mut qb, &mut has_where);
                push_source_predicate(&mut qb, acct);
            }
            if let Some((ts, id)) = cursor {
                push_glue(&mut qb, &mut has_where);
                push_cursor_predicate(&mut qb, ts, id, op);
            }
        }
    }

    qb.push(format!(
        " ORDER BY t.created_at {order}, t.id {order} LIMIT "
    ));
    qb.push_bind(params.limit + 1);

    let raw: Vec<PgRow> = qb.build().fetch_all(pool).await?;
    Ok(raw.iter().map(map_list_row).collect())
}

fn push_contract_union_arm<'q>(
    qb: &mut sqlx::QueryBuilder<'q, sqlx::Postgres>,
    table: &'static str,
    cid_strkey: &'q str,
    cursor: Option<(DateTime<Utc>, i64)>,
    op: &str,
) {
    qb.push("SELECT created_at, transaction_id FROM ");
    qb.push(table);
    qb.push(" WHERE contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ");
    qb.push_bind(cid_strkey);
    qb.push(")");
    if let Some((ts, id)) = cursor {
        qb.push(" AND (created_at, transaction_id) ");
        qb.push(op);
        qb.push(" (");
        qb.push_bind(ts);
        qb.push(", ");
        qb.push_bind(id);
        qb.push(")");
    }
}

fn push_source_predicate<'q>(
    qb: &mut sqlx::QueryBuilder<'q, sqlx::Postgres>,
    acct_strkey: &'q str,
) {
    qb.push(" t.source_id = (SELECT id FROM accounts WHERE account_id = ");
    qb.push_bind(acct_strkey);
    qb.push(")");
}

pub async fn lookup_hash_index(
    pool: &PgPool,
    hash_bytes: &[u8],
) -> Result<Option<HashIndexRow>, sqlx::Error> {
    let raw: Option<PgRow> =
        sqlx::query("SELECT created_at FROM transaction_hash_index WHERE hash = $1")
            .bind(hash_bytes)
            .fetch_optional(pool)
            .await?;

    Ok(raw.map(|r| HashIndexRow {
        created_at: r.get("created_at"),
    }))
}

pub async fn fetch_detail(
    pool: &PgPool,
    hash_bytes: &[u8],
    created_at: DateTime<Utc>,
) -> Result<Option<TxDetailRow>, sqlx::Error> {
    let raw: Option<PgRow> = sqlx::query(
        "SELECT \
            t.id, \
            encode(t.hash, 'hex')          AS hash, \
            t.ledger_sequence, \
            t.application_order, \
            a.account_id                   AS source_account, \
            t.fee_charged, \
            encode(t.inner_tx_hash, 'hex') AS inner_tx_hash, \
            t.successful, \
            t.operation_count, \
            t.has_soroban, \
            t.created_at, \
            t.parse_error \
         FROM transactions t \
         LEFT JOIN accounts a ON a.id = t.source_id \
         WHERE t.hash = $1 AND t.created_at = $2",
    )
    .bind(hash_bytes)
    .bind(created_at)
    .fetch_optional(pool)
    .await?;

    Ok(raw.map(|r| TxDetailRow {
        id: r.get("id"),
        hash: r.get("hash"),
        ledger_sequence: r.get("ledger_sequence"),
        application_order: r.get("application_order"),
        source_account: r.get("source_account"),
        fee_charged: r.get("fee_charged"),
        inner_tx_hash: r.get("inner_tx_hash"),
        successful: r.get("successful"),
        operation_count: r.get("operation_count"),
        has_soroban: r.get("has_soroban"),
        created_at: r.get("created_at"),
        parse_error: r.get("parse_error"),
    }))
}

pub async fn fetch_operations(
    pool: &PgPool,
    transaction_id: i64,
    created_at: DateTime<Utc>,
) -> Result<Vec<OpRow>, sqlx::Error> {
    // Task 0192: ORDER BY oa.application_order is the canonical contract.
    // `NULLS LAST, oa.id` keeps pre-task-0192 rows (where application_order
    // is NULL) in stable post-payload position so result-set order remains
    // deterministic even on partial-backfill DBs.
    let raw: Vec<PgRow> = sqlx::query(
        "SELECT \
            oa.id                           AS appearance_id, \
            op_type_name(oa.type)           AS type_name, \
            oa.type                         AS op_type, \
            src.account_id                  AS source_account, \
            dst.account_id                  AS destination_account, \
            sc.contract_id, \
            oa.asset_code, \
            iss.account_id                  AS asset_issuer, \
            array_remove(ARRAY[encode(oa.pool_id, 'hex')], NULL) AS pool_ids, \
            oa.application_order, \
            oa.ledger_sequence, \
            oa.created_at \
         FROM operations_appearances oa \
         LEFT JOIN accounts          src ON src.id = oa.source_id \
         LEFT JOIN accounts          dst ON dst.id = oa.destination_id \
         LEFT JOIN soroban_contracts sc  ON sc.id  = oa.contract_id \
         LEFT JOIN accounts          iss ON iss.id = oa.asset_issuer_id \
         WHERE oa.transaction_id = $1 AND oa.created_at = $2 \
         ORDER BY oa.application_order NULLS LAST, oa.id",
    )
    .bind(transaction_id)
    .bind(created_at)
    .fetch_all(pool)
    .await?;

    Ok(raw
        .iter()
        .map(|r| OpRow {
            appearance_id: r.get("appearance_id"),
            type_name: r.get("type_name"),
            op_type: r.get("op_type"),
            source_account: r.get("source_account"),
            destination_account: r.get("destination_account"),
            contract_id: r.get("contract_id"),
            asset_code: r.get("asset_code"),
            asset_issuer: r.get("asset_issuer"),
            pool_ids: r.get("pool_ids"),
            application_order: r.get("application_order"),
            ledger_sequence: r.get("ledger_sequence"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn fetch_participants(
    pool: &PgPool,
    transaction_id: i64,
    created_at: DateTime<Utc>,
) -> Result<Vec<String>, sqlx::Error> {
    let raw: Vec<PgRow> = sqlx::query(
        "SELECT a.account_id \
         FROM transaction_participants tp \
         JOIN accounts a ON a.id = tp.account_id \
         WHERE tp.transaction_id = $1 AND tp.created_at = $2 \
         ORDER BY a.account_id",
    )
    .bind(transaction_id)
    .bind(created_at)
    .fetch_all(pool)
    .await?;

    Ok(raw.iter().map(|r| r.get("account_id")).collect())
}

pub async fn fetch_event_appearances(
    pool: &PgPool,
    transaction_id: i64,
    created_at: DateTime<Utc>,
) -> Result<Vec<EventAppearanceRow>, sqlx::Error> {
    let raw: Vec<PgRow> = sqlx::query(
        "SELECT \
            sc.contract_id, \
            sea.ledger_sequence, \
            sea.created_at \
         FROM soroban_events_appearances sea \
         JOIN soroban_contracts sc ON sc.id = sea.contract_id \
         WHERE sea.transaction_id = $1 AND sea.created_at = $2 \
         ORDER BY sea.ledger_sequence, sc.contract_id",
    )
    .bind(transaction_id)
    .bind(created_at)
    .fetch_all(pool)
    .await?;

    Ok(raw
        .iter()
        .map(|r| EventAppearanceRow {
            contract_id: r.get("contract_id"),
            ledger_sequence: r.get("ledger_sequence"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn fetch_invocation_appearances(
    pool: &PgPool,
    transaction_id: i64,
    created_at: DateTime<Utc>,
) -> Result<Vec<InvocationAppearanceRow>, sqlx::Error> {
    let raw: Vec<PgRow> = sqlx::query(
        "SELECT \
            sc.contract_id, \
            caller.account_id    AS caller_account, \
            sia.ledger_sequence, \
            sia.created_at \
         FROM soroban_invocations_appearances sia \
         JOIN soroban_contracts sc      ON sc.id     = sia.contract_id \
         LEFT JOIN accounts      caller ON caller.id = sia.caller_id \
         WHERE sia.transaction_id = $1 AND sia.created_at = $2 \
         ORDER BY sia.ledger_sequence, sc.contract_id",
    )
    .bind(transaction_id)
    .bind(created_at)
    .fetch_all(pool)
    .await?;

    Ok(raw
        .iter()
        .map(|r| InvocationAppearanceRow {
            contract_id: r.get("contract_id"),
            caller_account: r.get("caller_account"),
            ledger_sequence: r.get("ledger_sequence"),
            created_at: r.get("created_at"),
        })
        .collect())
}
