//! Database queries for the assets endpoints.
//!
//! Aligned with canonical SQL `endpoint-queries/{08,09,10}_*.sql` (task 0167).
//! Two deliberate divergences: (1) `:id` resolution stays at the API layer
//! (3 fetch_by_* paths, no surrogate-first single-SQL); (2) `/transactions`
//! is one OR'd query instead of canonical's split A/B variants. Both produce
//! the same result.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::common::cursor::{Direction, TsIdCursor, direction_sql};

#[derive(Debug, Clone)]
pub struct AssetRow {
    pub asset_type: i16,
    /// Pre-decoded via `token_asset_type_name()` SQL helper. `None` only
    /// when the discriminant is outside the schema CHECK range — defensive
    /// against future schema drift.
    pub asset_type_name: Option<String>,
    pub asset_code: Option<String>,
    /// Already resolved through `accounts.account_id` join.
    pub issuer: Option<String>,
    /// Already resolved through `soroban_contracts.contract_id` join.
    pub contract_id: Option<String>,
    pub name: Option<String>,
    pub total_supply: Option<String>,
    pub holder_count: Option<i32>,
    pub icon_url: Option<String>,
    /// `soroban_contracts.deployed_at_ledger` — populated for SAC and
    /// Soroban-native rows; `None` for native and classic_credit.
    pub deployed_at_ledger: Option<i64>,
    /// `accounts.home_domain` for the issuer, used as the SEP-1 lookup
    /// key in `get_asset` runtime enrichment (task 0188). `None` for
    /// native, no-issuer, and issuer accounts that did not set
    /// `home_domain` on-chain.
    pub issuer_home_domain: Option<String>,
    /// Surrogate key columns — cursor keyset only, never on the wire. These
    /// are the 4-tuple CH orders `assets` by `(asset_type, asset_code,
    /// issuer_id, contract_id)`; `0` / `''` stand in for "absent" (native has
    /// no issuer_id, classic-credit has no contract_id), matching CH defaults.
    pub issuer_id: i64,
    pub contract_surrogate_id: i64,
}

#[derive(Debug)]
pub struct AssetTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub successful: bool,
    pub fee_charged: i64,
    pub created_at: DateTime<Utc>,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
}

/// Pagination payload for `GET /v1/assets`. The numeric surrogate was dropped
/// (PR #175 / the PG→CH composite move), so the keyset walks the natural
/// identity 4-tuple `(asset_type, asset_code, issuer_id, contract_id)` — the
/// exact `ORDER BY` of the CH `assets` table, so the cursor is
/// datasource-agnostic. `issuer_id` / `contract_id` are the surrogate key
/// columns (`0` = absent); `asset_code` is `''` for native.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetKeyCursor {
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub contract_id: i64,
}

pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<AssetKeyCursor>,
    pub asset_type: Option<i16>,
    /// Raw substring (no `%` / `_` from the caller). The SQL builder
    /// wraps it in `%...%` for the trigram match.
    pub asset_code: Option<String>,
}

fn push_glue(qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, has_where: &mut bool) {
    qb.push(if *has_where { " AND" } else { " WHERE" });
    *has_where = true;
}

const ASSET_SELECT: &str = "SELECT \
     token_asset_type_name(a.asset_type) AS asset_type_name, \
     a.asset_type AS asset_type, \
     a.asset_code, \
     iss.account_id AS issuer, \
     iss.home_domain AS issuer_home_domain, \
     sc.contract_id, \
     a.name, \
     a.total_supply::text AS total_supply, \
     a.holder_count, \
     a.icon_url, \
     sc.deployed_at_ledger AS deployed_at_ledger, \
     COALESCE(a.issuer_id, 0)   AS issuer_id_key, \
     COALESCE(a.contract_id, 0) AS contract_id_key \
     FROM assets a \
     LEFT JOIN accounts iss ON iss.id = a.issuer_id \
     LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id";

fn map_asset_row(r: &PgRow) -> AssetRow {
    AssetRow {
        asset_type: r.get("asset_type"),
        asset_type_name: r.get("asset_type_name"),
        asset_code: r.get("asset_code"),
        issuer: r.get("issuer"),
        contract_id: r.get("contract_id"),
        name: r.get("name"),
        total_supply: r.get("total_supply"),
        holder_count: r.get("holder_count"),
        icon_url: r.get("icon_url"),
        deployed_at_ledger: r.get("deployed_at_ledger"),
        issuer_home_domain: r.get("issuer_home_domain"),
        issuer_id: r.get("issuer_id_key"),
        contract_surrogate_id: r.get("contract_id_key"),
    }
}

pub async fn fetch_list(
    pool: &PgPool,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<AssetRow>, sqlx::Error> {
    let (op, order) = direction_sql(direction);

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(ASSET_SELECT);
    let mut has_where = false;

    if let Some(t) = params.asset_type {
        push_glue(&mut qb, &mut has_where);
        qb.push(" a.asset_type = ");
        qb.push_bind(t);
    }
    if let Some(code) = &params.asset_code {
        // Substring trigram match — leading `%` defeats btree but is served
        // by `idx_assets_code_trgm` (GIN gin_trgm_ops). The wrap happens here
        // so callers pass the raw substring (not a LIKE pattern).
        push_glue(&mut qb, &mut has_where);
        qb.push(" a.asset_code ILIKE '%' || ");
        qb.push_bind(code.as_str());
        qb.push(" || '%'");
    }
    if let Some(cursor) = &params.cursor {
        push_glue(&mut qb, &mut has_where);
        qb.push(
            " (a.asset_type, COALESCE(a.asset_code, ''), COALESCE(a.issuer_id, 0), \
              COALESCE(a.contract_id, 0)) "
                .to_string(),
        );
        qb.push(format!("{op} ("));
        qb.push_bind(cursor.asset_type);
        qb.push(", ");
        qb.push_bind(cursor.asset_code.clone());
        qb.push(", ");
        qb.push_bind(cursor.issuer_id);
        qb.push(", ");
        qb.push_bind(cursor.contract_id);
        qb.push(")");
    }

    qb.push(format!(
        " ORDER BY a.asset_type {order}, COALESCE(a.asset_code, '') {order}, \
          COALESCE(a.issuer_id, 0) {order}, COALESCE(a.contract_id, 0) {order} LIMIT "
    ));
    // `params.limit` is the handler's `fetch_limit()` (already the peek +1).
    qb.push_bind(params.limit);

    let raw: Vec<PgRow> = qb.build().fetch_all(pool).await?;
    Ok(raw.iter().map(map_asset_row).collect())
}

/// Resolve the classic native XLM singleton (`asset_type = 0`). Native has no
/// composite identity (no contract_id, no code/issuer per `ck_assets_identity`),
/// so it is addressed by the reserved `/assets/native` token rather than a
/// StrKey or CODE-ISSUER pair.
pub async fn fetch_native(pool: &PgPool) -> Result<Option<AssetRow>, sqlx::Error> {
    let sql = format!("{ASSET_SELECT} WHERE a.asset_type = 0 LIMIT 1");
    let raw: Option<PgRow> = sqlx::query(&sql).fetch_optional(pool).await?;
    Ok(raw.as_ref().map(map_asset_row))
}

pub async fn fetch_by_contract_id(
    pool: &PgPool,
    contract_id: &str,
) -> Result<Option<AssetRow>, sqlx::Error> {
    let sql = format!("{ASSET_SELECT} WHERE sc.contract_id = $1");
    let raw: Option<PgRow> = sqlx::query(&sql)
        .bind(contract_id)
        .fetch_optional(pool)
        .await?;
    Ok(raw.as_ref().map(map_asset_row))
}

pub async fn fetch_by_code_issuer(
    pool: &PgPool,
    asset_code: &str,
    issuer: &str,
) -> Result<Option<AssetRow>, sqlx::Error> {
    let sql = format!("{ASSET_SELECT} WHERE a.asset_code = $1 AND iss.account_id = $2 LIMIT 1");
    let raw: Option<PgRow> = sqlx::query(&sql)
        .bind(asset_code)
        .bind(issuer)
        .fetch_optional(pool)
        .await?;
    Ok(raw.as_ref().map(map_asset_row))
}

/// Identity slice used by [`fetch_transactions`] to compose its predicate.
/// `fetch_transactions` enforces non-empty identity — see its own guard —
/// but the upstream [`asset_predicate_present`] check is the canonical
/// short-circuit for native-XLM and friends.
pub struct AssetIdentity<'a> {
    pub asset_code: Option<&'a str>,
    pub issuer: Option<&'a str>,
    pub contract_id: Option<&'a str>,
}

/// Per-`asset_type` predicate composition (ADR 0037 §223–258 + 0038):
///   classic_credit         → `(asset_code, asset_issuer_id)`
///   sac (classic-wrap)     → classic identity OR `contract_id`
///   sac (native-wrap)      → `contract_id` only
///   soroban                → `contract_id` only
///   native                 → defended below: empty identity → empty result
///
/// SQL shape mirrors canonical `10_get_assets_transactions.sql`:
/// drive from `operations_appearances` first (matched_ops CTE) so the
/// partial indexes (`idx_ops_app_asset`, `idx_ops_app_contract`) carry
/// the cursor-bounded scan; pre-LIMIT to `limit*4` caps duplicate-tx
/// blowup before joining to `transactions`. The final SELECT then
/// sees a small de-duplicated set instead of asking `DISTINCT` to dedupe
/// after the join.
pub async fn fetch_transactions(
    pool: &PgPool,
    identity: &AssetIdentity<'_>,
    limit: i64,
    cursor: Option<&TsIdCursor>,
    direction: Direction,
) -> Result<Vec<AssetTxRow>, sqlx::Error> {
    let has_classic = identity.asset_code.is_some() && identity.issuer.is_some();
    let has_contract = identity.contract_id.is_some();
    let (op, order) = direction_sql(direction);

    // Defensive: never emit `WHERE ()`. The upstream handler routes through
    // `asset_predicate_present`, but `pub fn` callers in the future could
    // miss it — short-circuit here so a misuse degrades to empty rather
    // than to an invalid SQL string.
    if !has_classic && !has_contract {
        return Ok(Vec::new());
    }

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "WITH matched_ops AS ( \
             SELECT DISTINCT ON (oa.created_at, oa.transaction_id) \
                    oa.transaction_id, \
                    oa.created_at \
             FROM operations_appearances oa \
             WHERE (",
    );

    let mut wrote_branch = false;
    if has_classic {
        qb.push("(oa.asset_code = ");
        qb.push_bind(identity.asset_code.expect("guarded above"));
        qb.push(" AND oa.asset_issuer_id = (SELECT id FROM accounts WHERE account_id = ");
        qb.push_bind(identity.issuer.expect("guarded above"));
        qb.push("))");
        wrote_branch = true;
    }
    if has_contract {
        if wrote_branch {
            qb.push(" OR ");
        }
        qb.push("(oa.contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ");
        qb.push_bind(identity.contract_id.expect("guarded above"));
        qb.push("))");
    }
    qb.push(")");

    if let Some(c) = cursor {
        qb.push(format!(" AND (oa.created_at, oa.transaction_id) {op} ("));
        qb.push_bind(c.ts);
        qb.push(", ");
        qb.push_bind(c.id);
        qb.push(")");
    }

    qb.push(format!(
        " ORDER BY oa.created_at {order}, oa.transaction_id {order}, oa.id \
          LIMIT "
    ));
    qb.push_bind(limit * 4);
    qb.push(format!(
        ") \
         SELECT t.id, encode(t.hash, 'hex') AS hash, t.ledger_sequence, \
                a.account_id AS source_account, t.successful, t.fee_charged, \
                t.created_at, t.operation_count, t.has_soroban, \
                COALESCE(ops.operation_types, ARRAY[]::text[]) AS operation_types \
         FROM matched_ops m \
         JOIN transactions t \
              ON t.id = m.transaction_id AND t.created_at = m.created_at \
         JOIN accounts a ON a.id = t.source_id \
         LEFT JOIN LATERAL ( \
             SELECT array_agg(DISTINCT op_type_name(oa2.type) \
                              ORDER BY op_type_name(oa2.type)) AS operation_types \
             FROM operations_appearances oa2 \
             WHERE oa2.transaction_id = t.id \
               AND oa2.created_at     = t.created_at \
         ) ops ON TRUE \
         ORDER BY t.created_at {order}, t.id {order} \
         LIMIT "
    ));
    // `limit` is the handler's `fetch_limit()` (already the peek +1).
    qb.push_bind(limit);

    let raw: Vec<PgRow> = qb.build().fetch_all(pool).await?;
    Ok(raw
        .iter()
        .map(|r| AssetTxRow {
            id: r.get("id"),
            hash: r.get("hash"),
            ledger_sequence: r.get("ledger_sequence"),
            source_account: r.get("source_account"),
            successful: r.get("successful"),
            fee_charged: r.get("fee_charged"),
            created_at: r.get("created_at"),
            operation_count: r.get("operation_count"),
            has_soroban: r.get("has_soroban"),
            operation_types: r.get("operation_types"),
        })
        .collect())
}

/// `false` for native XLM (no DB-side identity referenced by ops). Caller
/// short-circuits with an empty page so [`fetch_transactions`] never emits
/// a degenerate `WHERE ()` SQL.
pub fn asset_predicate_present(identity: &AssetIdentity<'_>) -> bool {
    let has_classic = identity.asset_code.is_some() && identity.issuer.is_some();
    let has_contract = identity.contract_id.is_some();
    has_classic || has_contract
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::cursor;

    /// The composite list cursor must survive the opaque encode→decode for
    /// every asset shape — especially the native boundary, where `asset_code`
    /// is the empty string and `issuer_id`/`contract_id` are the `0` sentinel.
    /// A serde drift there would silently break pagination at the native row.
    #[test]
    fn asset_key_cursor_round_trips_including_native_boundary() {
        let cases = [
            // native: empty code + zero surrogates
            AssetKeyCursor {
                asset_type: 0,
                asset_code: String::new(),
                issuer_id: 0,
                contract_id: 0,
            },
            // classic credit: code + issuer surrogate, no contract
            AssetKeyCursor {
                asset_type: 1,
                asset_code: "USDC".to_string(),
                issuer_id: 42,
                contract_id: 0,
            },
            // SAC native-wrap: contract only, empty code
            AssetKeyCursor {
                asset_type: 2,
                asset_code: String::new(),
                issuer_id: 0,
                contract_id: 99,
            },
        ];
        for c in cases {
            let encoded = cursor::encode(&c, Direction::Next);
            let (dir, decoded): (Direction, AssetKeyCursor) = cursor::decode(&encoded).unwrap();
            assert_eq!(dir, Direction::Next);
            assert_eq!(decoded.asset_type, c.asset_type);
            assert_eq!(decoded.asset_code, c.asset_code);
            assert_eq!(decoded.issuer_id, c.issuer_id);
            assert_eq!(decoded.contract_id, c.contract_id);
        }
    }
}
