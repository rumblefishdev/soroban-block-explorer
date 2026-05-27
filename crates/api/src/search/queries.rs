//! Database queries backing `GET /v1/search`.
//!
//! Implementation of the canonical SQL in
//! `docs/architecture/database-schema/endpoint-queries/22_get_search.sql`.
//!
//! Single query: [`fetch_search`] runs the union-of-CTEs broad-search
//! statement with the per-bucket `:include_*` flags resolved from the
//! optional `?type=` filter. The handler decides between a `Redirect`
//! and a `Results` response by counting the returned rows — a singleton
//! row whose entity type fits the redirect wire shape is rendered as
//! `SearchResponse::Redirect`; everything else returns as
//! `SearchResponse::Results` (option C refactor — task 0271).

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
/// Six flags, one per `EntityType`. Under option C every entity type
/// has its own broad-search CTE — `transaction` and `pool` match by
/// exact `BYTEA(32)` on `hash_bytes` (and so produce at most one row);
/// `account` and `contract` match by `LIKE` prefix on the strkey text
/// column; `asset` and `nft` match by `ILIKE` substring on the text
/// label. Whichever bucket fires, the handler treats a singleton
/// result as a redirect when the entity type is redirect-eligible
/// (see `SearchRedirect::from_hit`).
#[derive(Debug, Clone, Copy)]
pub struct IncludeFlags {
    pub transaction: bool,
    pub contract: bool,
    pub asset: bool,
    pub account: bool,
    pub nft: bool,
    pub pool: bool,
}

impl IncludeFlags {
    pub fn all() -> Self {
        Self {
            transaction: true,
            contract: true,
            asset: true,
            account: true,
            nft: true,
            pool: true,
        }
    }

    pub fn none() -> Self {
        Self {
            transaction: false,
            contract: false,
            asset: false,
            account: false,
            nft: false,
            pool: false,
        }
    }

    pub fn enable(&mut self, t: EntityType) {
        match t {
            EntityType::Transaction => self.transaction = true,
            EntityType::Contract => self.contract = true,
            EntityType::Asset => self.asset = true,
            EntityType::Account => self.account = true,
            EntityType::Nft => self.nft = true,
            EntityType::Pool => self.pool = true,
        }
    }
}

// ---------------------------------------------------------------------------
// Broad search
// ---------------------------------------------------------------------------

/// Run the canonical `22_get_search.sql` UNION of six narrow CTEs and
/// return the rows partitioned by `entity_type`. The caller groups
/// these into [`SearchGroups`](super::dto::SearchGroups) for the JSON
/// response, and synthesizes `SearchResponse::Redirect` when the
/// returned row count is exactly one (option C — task 0271).
pub async fn fetch_search(
    pool: &PgPool,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, sqlx::Error> {
    // Broad-search UNION across all six entity buckets. Option C
    // collapses the previous two-path design (redirect short-circuit +
    // broad fallback) into a single SQL — the handler counts rows and
    // synthesizes `SearchResponse::Redirect` when exactly one row
    // returns and its entity type is redirect-eligible (see
    // `SearchRedirect::from_hit`).
    //
    // CTE shape map:
    //   tx_hits      — exact match on BYTEA(32) hash via `hash_bytes`
    //                  (singleton ⇒ Redirect via tx detail page)
    //   pool_hits    — exact match on BYTEA(32) pool_id via `hash_bytes`
    //                  (singleton ⇒ Redirect via pool detail page;
    //                  partial-L-prefix support deferred — see 0271
    //                  Future Work for the CH-era denorm column)
    //   contract_hits — `LIKE 'PREFIX%'` on contract_id text column
    //                   OR full-text search when no prefix supplied
    //   asset_hits    — `ILIKE '%SUBSTR%'` on asset_code text
    //   account_hits  — `LIKE 'PREFIX%'` on account_id text column
    //   nft_hits      — `ILIKE '%SUBSTR%'` on n.name text + JOIN to
    //                   soroban_contracts for the C-strkey routing key
    let sql = r#"
        WITH
        tx_hits AS (
            -- Composite-PK lookup over `(hash, created_at)` via the
            -- partition-pruned index. JOIN `transactions` for the
            -- richer `successful` enrichment surfaced on the redirect
            -- payload.
            SELECT
                'transaction'::text                AS entity_type,
                encode(thi.hash, 'hex')            AS identifier,
                ''::text                           AS label,
                NULL::bigint                       AS surrogate_id,
                t.successful                       AS successful,
                thi.created_at                     AS last_activity_at,
                NULL::varchar                      AS contract_id,
                NULL::varchar                      AS token_id
            FROM transaction_hash_index thi
            LEFT JOIN transactions t
              ON t.hash = thi.hash
             AND t.created_at = thi.created_at
            WHERE $5 = TRUE
              AND $2 IS NOT NULL
              AND thi.hash = $2
            LIMIT $4
        ),
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
            WHERE $6 = TRUE
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
            WHERE $7 = TRUE
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
            WHERE $8 = TRUE
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
            WHERE $9 = TRUE
              AND n.name IS NOT NULL
              AND n.name ILIKE '%' || $1 || '%'
            LIMIT $4
        ),
        pool_hits AS (
            -- Exact match on BYTEA(32) `pool_id` via `hash_bytes` from
            -- the classifier (full L-strkey decode). Partial-L prefix
            -- matching requires a denormalised L-strkey text column —
            -- deferred to the CH-era follow-up (see 0271 Future Work).
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
            WHERE $10 = TRUE
              AND $2 IS NOT NULL
              AND lp.pool_id = $2
            LIMIT $4
        )
        SELECT entity_type, identifier, label, surrogate_id, successful, last_activity_at, contract_id, token_id FROM tx_hits
        UNION ALL
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
        .bind(include.transaction)
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
