//! Database queries backing `GET /v1/search`.
//!
//! Implementation of the canonical SQL in
//! `docs/architecture/database-schema/endpoint-queries/22_get_search.sql`.
//!
//! Two queries:
//!
//! * [`fetch_redirect`] — exact-match short-circuit when `q` is a
//!   fully-typed entity id (64-hex hash, full G-StrKey, full C-StrKey).
//!   Returns `Some(EntityType, identifier)` for the first table that
//!   matches, in priority order, or `None` for fall-through to broad
//!   search.
//! * [`fetch_search`] — runs the union-of-CTEs broad-search statement
//!   with the per-bucket `:include_*` flags resolved from the optional
//!   `?type=` filter.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use super::classifier::Classified;
use super::dto::{EntityType, SearchHit};
use crate::common::strkey::pool_id_hex_to_strkey;

// ---------------------------------------------------------------------------
// Type-filter map
// ---------------------------------------------------------------------------

/// Per-entity inclusion flags for the broad-search UNION. Defaults to
/// "include everything"; a caller-supplied `?type=foo,bar` filter
/// narrows it before the query runs.
///
/// `transaction` is NOT in this struct: tx broad search matches by
/// exact `BYTEA(32)` on `hash_bytes`, identical to what
/// `fetch_redirect` checks first. Any non-empty broad result would
/// have already triggered a redirect, so the broad-search CTE was
/// dead code and was dropped. The `?type=transaction` filter token
/// is still accepted by the handler (parity with the wire
/// discriminator) but is a no-op in the broad branch — the redirect
/// branch is the only path that surfaces transactions. Pool stays
/// here because backlog task 0271 will denormalise an `L…` strkey
/// column onto `liquidity_pools` and switch this bucket to partial
/// prefix matching (parity with account / contract).
#[derive(Debug, Clone, Copy)]
pub struct IncludeFlags {
    pub contract: bool,
    pub asset: bool,
    pub account: bool,
    pub nft: bool,
    pub pool: bool,
}

impl IncludeFlags {
    pub fn all() -> Self {
        Self {
            contract: true,
            asset: true,
            account: true,
            nft: true,
            pool: true,
        }
    }

    pub fn none() -> Self {
        Self {
            contract: false,
            asset: false,
            account: false,
            nft: false,
            pool: false,
        }
    }

    pub fn enable(&mut self, t: EntityType) {
        match t {
            EntityType::Contract => self.contract = true,
            EntityType::Asset => self.asset = true,
            EntityType::Account => self.account = true,
            EntityType::Nft => self.nft = true,
            EntityType::Pool => self.pool = true,
            // `transaction` is redirect-only — no broad-search bucket.
            // Filter token accepted (handler does not reject) but
            // produces no rows in `Results`.
            EntityType::Transaction => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Redirect short-circuit
// ---------------------------------------------------------------------------

/// Look up the first entity that matches `q` exactly. Used when the
/// classifier flags `q` as fully-typed — frontend wants to skip the
/// dropdown and navigate directly.
///
/// Priority (matches the human "what was the user looking for?"
/// expectation when an id-shape value is ambiguous):
///   1. transaction hash (32-byte BYTEA)
///   2. liquidity pool id (also 32-byte BYTEA — same `hash_bytes`)
///   3. account StrKey (full 56 chars, `G…`)
///   4. soroban contract StrKey (full 56 chars, `C…`)
///
/// Returns `None` if no exact row matches; the caller falls through to
/// broad search so the user still sees suggestions instead of a 404.
/// Redirect-path result row. Carries the entity coordinates plus the
/// same `successful` + `last_activity_at` enrichment as a `SearchHit`
/// so the frontend can render a richer dropdown row when it presents
/// the redirect as a clickable hit instead of auto-navigating.
pub struct RedirectRow {
    pub entity_type: EntityType,
    pub entity_id: String,
    pub successful: Option<bool>,
    pub last_activity_at: Option<DateTime<Utc>>,
}

pub async fn fetch_redirect(
    pool: &PgPool,
    classified: &Classified,
) -> Result<Option<RedirectRow>, sqlx::Error> {
    if !classified.is_fully_typed() {
        return Ok(None);
    }

    if let Some(bytes) = &classified.hash_bytes {
        // Same `(hash, created_at)` join as the broad-search tx_hits CTE
        // — composite PK lookup, partition-pruned.
        let tx: Option<(String, Option<bool>, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT encode(thi.hash, 'hex'),
                   t.successful,
                   thi.created_at
            FROM transaction_hash_index thi
            LEFT JOIN transactions t
              ON t.hash = thi.hash
             AND t.created_at = thi.created_at
            WHERE thi.hash = $1
            "#,
        )
        .bind(bytes.as_slice())
        .fetch_optional(pool)
        .await?;
        if let Some((id, successful, last_activity_at)) = tx {
            return Ok(Some(RedirectRow {
                entity_type: EntityType::Transaction,
                entity_id: id,
                successful,
                last_activity_at: Some(last_activity_at),
            }));
        }

        let pool_row: Option<(String,)> =
            sqlx::query_as("SELECT encode(pool_id, 'hex') FROM liquidity_pools WHERE pool_id = $1")
                .bind(bytes.as_slice())
                .fetch_optional(pool)
                .await?;
        if let Some((id,)) = pool_row {
            return Ok(Some(RedirectRow {
                entity_type: EntityType::Pool,
                // Wire form is the canonical `L…` strkey (per ADR 0008 /
                // task 0264). DB stores raw BYTEA(32); the SELECT above
                // encoded as hex, which the helper re-encodes as strkey.
                entity_id: pool_id_hex_to_strkey(&id),
                successful: None,
                last_activity_at: None,
            }));
        }
        return Ok(None);
    }

    if let Some(strkey) = &classified.strkey_prefix
        && strkey.len() == 56
    {
        match strkey.as_bytes()[0] {
            b'G' => {
                let row: Option<(String,)> =
                    sqlx::query_as("SELECT account_id FROM accounts WHERE account_id = $1")
                        .bind(strkey)
                        .fetch_optional(pool)
                        .await?;
                if let Some((id,)) = row {
                    return Ok(Some(RedirectRow {
                        entity_type: EntityType::Account,
                        entity_id: id,
                        successful: None,
                        last_activity_at: None,
                    }));
                }
            }
            b'C' => {
                let row: Option<(String,)> = sqlx::query_as(
                    "SELECT contract_id FROM soroban_contracts WHERE contract_id = $1",
                )
                .bind(strkey)
                .fetch_optional(pool)
                .await?;
                if let Some((id,)) = row {
                    return Ok(Some(RedirectRow {
                        entity_type: EntityType::Contract,
                        entity_id: id,
                        successful: None,
                        last_activity_at: None,
                    }));
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Broad search
// ---------------------------------------------------------------------------

/// Run the canonical `22_get_search.sql` UNION of six narrow CTEs and
/// return the rows partitioned by `entity_type`. The caller groups
/// these into [`SearchGroups`](super::dto::SearchGroups) for the JSON
/// response.
pub async fn fetch_search(
    pool: &PgPool,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, sqlx::Error> {
    // Broad-search UNION across the five `?type=`-filterable buckets.
    // `transaction` was dropped — tx broad matched by exact
    // `BYTEA(32) hash_bytes` which is what `fetch_redirect` already
    // checks first; any non-empty broad result would have triggered a
    // redirect, so the CTE was dead code. The `?type=transaction`
    // filter token is still accepted by the handler (parity with the
    // wire discriminator) but the broad-search query has no rows to
    // return for it — the redirect branch is the only path that
    // surfaces transactions.
    //
    // `pool_hits` matches the same way (exact `hash_bytes`) and is
    // similarly redundant today, but stays here as scaffold for task
    // 0271 which will denormalise an `L…` strkey column onto
    // `liquidity_pools` and switch this CTE to partial `LIKE 'L%'`
    // prefix matching (parity with account / contract).
    let sql = r#"
        WITH
        contract_hits AS (
            SELECT
                'contract'::text          AS entity_type,
                sc.contract_id            AS identifier,
                COALESCE(sc.name, '')              AS label,
                sc.id                     AS surrogate_id,
                NULL::bool                AS successful,
                NULL::timestamptz         AS last_activity_at,
                NULL::varchar             AS contract_id,
                NULL::varchar             AS token_id
            FROM soroban_contracts sc
            WHERE $5 = TRUE
              AND (
                      ( $3 IS NOT NULL AND sc.contract_id LIKE $3 || '%' )
                   OR ( $3 IS NULL     AND sc.search_vector @@ plainto_tsquery('simple', $1) )
                  )
            LIMIT $4
        ),
        asset_hits AS (
            SELECT
                'asset'::text                       AS entity_type,
                COALESCE(a.asset_code, 'XLM')       AS identifier,
                token_asset_type_name(a.asset_type) AS label,
                a.id::bigint                        AS surrogate_id,
                NULL::bool                          AS successful,
                NULL::timestamptz                   AS last_activity_at,
                NULL::varchar                       AS contract_id,
                NULL::varchar                       AS token_id
            FROM assets a
            WHERE $6 = TRUE
              AND (
                      (a.asset_code IS NOT NULL AND a.asset_code ILIKE '%' || $1 || '%')
                   OR (a.asset_type = 0 AND ($1 ILIKE 'xlm' OR $1 ILIKE 'native'))
                  )
            LIMIT $4
        ),
        account_hits AS (
            SELECT
                'account'::text         AS entity_type,
                a.account_id            AS identifier,
                COALESCE(a.home_domain, '') AS label,
                a.id                    AS surrogate_id,
                NULL::bool              AS successful,
                NULL::timestamptz       AS last_activity_at,
                NULL::varchar           AS contract_id,
                NULL::varchar           AS token_id
            FROM accounts a
            WHERE $7 = TRUE
              AND $3 IS NOT NULL
              AND a.account_id LIKE $3 || '%'
            LIMIT $4
        ),
        nft_hits AS (
            -- JOIN soroban_contracts to project the C-strkey + token_id
            -- composite that the FE needs to route to
            -- `/nfts/:contract_id/:token_id` (per ADR 0030 / task 0264
            -- Phase 8a). `n.contract_id` is the surrogate FK; the actual
            -- public C-strkey lives in `soroban_contracts.contract_id`.
            SELECT
                'nft'::text                          AS entity_type,
                n.name                               AS identifier,
                COALESCE(n.collection_name, '')      AS label,
                n.id::bigint                         AS surrogate_id,
                NULL::bool                           AS successful,
                NULL::timestamptz                    AS last_activity_at,
                sc.contract_id                       AS contract_id,
                n.token_id                           AS token_id
            FROM nfts n
            JOIN soroban_contracts sc ON sc.id = n.contract_id
            WHERE $8 = TRUE
              AND n.name IS NOT NULL
              AND n.name ILIKE '%' || $1 || '%'
            LIMIT $4
        ),
        pool_hits AS (
            -- TODO(0271): switch this predicate to
            --   lp.pool_id_strkey LIKE $3 || '%'
            -- once task 0271 adds the denormalised L-strkey text
            -- column. The current `lp.pool_id = $2` predicate is
            -- identical to `fetch_redirect`'s — non-empty broad
            -- result here is unreachable in practice (redirect path
            -- fires first), so the CTE is dead code today and stays
            -- only as scaffold for the upcoming partial-prefix
            -- migration. See backlog task 0271.
            SELECT
                'pool'::text                AS entity_type,
                encode(lp.pool_id, 'hex')   AS identifier,
                (
                    COALESCE(lp.asset_a_code, 'XLM')
                    || ' / '
                    || COALESCE(lp.asset_b_code, 'XLM')
                )::text                     AS label,
                NULL::bigint                AS surrogate_id,
                NULL::bool                  AS successful,
                NULL::timestamptz           AS last_activity_at,
                NULL::varchar               AS contract_id,
                NULL::varchar               AS token_id
            FROM liquidity_pools lp
            WHERE $9 = TRUE
              AND $2 IS NOT NULL
              AND lp.pool_id = $2
            LIMIT $4
        )
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM contract_hits
        UNION ALL
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM asset_hits
        UNION ALL
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM account_hits
        UNION ALL
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM nft_hits
        UNION ALL
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM pool_hits
    "#;

    let rows = sqlx::query(sql)
        .bind(q)
        .bind(classified.hash_bytes.as_deref())
        .bind(classified.strkey_prefix.as_deref())
        .bind(per_group_limit)
        .bind(include.contract)
        .bind(include.asset)
        .bind(include.account)
        .bind(include.nft)
        .bind(include.pool)
        .fetch_all(pool)
        .await?;

    let hits = rows
        .into_iter()
        .filter_map(|row| {
            let entity_type: String = row.get("entity_type");
            let parsed = match EntityType::parse(&entity_type) {
                Some(p) => p,
                None => {
                    tracing::error!(
                        entity_type = entity_type.as_str(),
                        "search SQL emitted unknown entity_type literal — \
                         22_get_search.sql out of sync with EntityType enum",
                    );
                    return None;
                }
            };
            let identifier: String = row.get("identifier");
            let label: String = row.get("label");
            let surrogate_id: Option<i64> = row.get("surrogate_id");
            let successful: Option<bool> = row.get("successful");
            let last_activity_at: Option<DateTime<Utc>> = row.get("last_activity_at");
            let contract_id: Option<String> = row.get("contract_id");
            let token_id: Option<String> = row.get("token_id");
            // Pool identifier on the wire is the canonical `L…` strkey
            // (per ADR 0008 / task 0264). The CTE projects raw hex from
            // `BYTEA(32)` — convert at the row-mapper boundary.
            let identifier = if matches!(parsed, EntityType::Pool) {
                pool_id_hex_to_strkey(&identifier)
            } else {
                identifier
            };
            Some((
                entity_type,
                SearchHit {
                    entity_type: parsed,
                    identifier,
                    label,
                    surrogate_id,
                    successful,
                    last_activity_at,
                    contract_id,
                    token_id,
                },
            ))
        })
        .collect();

    Ok(hits)
}
