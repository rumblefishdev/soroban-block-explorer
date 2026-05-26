//! Database queries for the NFT endpoints.
//!
//! Aligned with canonical SQL `endpoint-queries/{15,16,17}_*.sql`.
//!
//! Row mapping note: NFT row shapes are 1:1 with the wire DTOs
//! (`NftItem`, `NftTransferItem`) — no fields are dropped or restructured
//! between DB and JSON, unlike the assets / pools modules where Row and
//! Item diverge (Asset Row carries an extra `deployed_at_ledger` column
//! used only by detail; Pool Row uses flat asset columns that the
//! handler folds into a nested JSONB shape). To avoid pure pass-through
//! mappers, we read straight into the wire DTOs here.

use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::common::cursor::{Direction, direction_sql};

use super::dto::{NftIdCursor, NftItem, NftTransferCursor, NftTransferItem};

pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<NftIdCursor>,
    pub filter_collection: Option<String>,
    pub filter_contract_id: Option<String>,
    /// Raw substring (no `%` / `_` from caller). SQL composes `%...%`.
    pub filter_name: Option<String>,
}

fn map_nft_item(r: &PgRow) -> NftItem {
    NftItem {
        id: r.get("id"),
        contract_id: r.get("contract_id"),
        token_id: r.get("token_id"),
        collection_name: r.get("collection_name"),
        name: r.get("name"),
        media_url: r.get("media_url"),
        minted_at_ledger: r.get("minted_at_ledger"),
        owner_account: r.get("owner_account"),
        last_seen_ledger: r.get("last_seen_ledger"),
    }
}

/// `GET /v1/nfts` — paginated list with optional filters.
///
/// The contract-id resolve uses a CTE so it runs once even when the
/// planner materialises `idx_nfts_collection` / `idx_nfts_name_trgm`.
///
/// `filter_name` is wrapped in `'%' || $5 || '%'` for the trigram match.
/// We do NOT add an `ESCAPE` clause here: the upstream handler rejects
/// values containing literal `%` / `_` with a 400 envelope (mirrors the
/// `assets` handler convention, see `assets/handlers.rs`). Keeping the
/// reject at the handler boundary keeps the SQL plan textually identical
/// to canonical `15_get_nfts_list.sql` and avoids the maintenance burden
/// of carrying an escape character through every callsite. If a future
/// caller bypasses the handler, the worst-case impact is a wider trigram
/// match — not SQL injection (the value is always bound, never
/// concatenated).
pub async fn fetch_list(
    pool: &PgPool,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<NftItem>, sqlx::Error> {
    let cur_id: Option<i32> = params.cursor.as_ref().map(|c| c.id);
    let (op, order) = direction_sql(direction);

    // Static query plan per direction. SQL fragments `{op}` and `{order}`
    // are hardcoded literals (`<`/`>`, `DESC`/`ASC`) — no injection risk.
    let sql = format!(
        r#"
        WITH ct AS (
            SELECT id
            FROM soroban_contracts
            WHERE $4::varchar IS NOT NULL
              AND contract_id = $4
        )
        SELECT
            n.id,
            sc.contract_id        AS contract_id,
            n.token_id,
            n.collection_name,
            n.name,
            n.media_url,
            n.minted_at_ledger,
            own.account_id        AS owner_account,
            n.current_owner_ledger AS last_seen_ledger
        FROM nfts n
        JOIN      soroban_contracts sc  ON sc.id = n.contract_id
        LEFT JOIN accounts          own ON own.id = n.current_owner_id
        WHERE
            ($2::int     IS NULL OR n.id {op} $2)
            AND ($3::varchar IS NULL OR n.collection_name = $3)
            AND ($4::varchar IS NULL OR n.contract_id = (SELECT id FROM ct))
            AND ($5::text    IS NULL OR n.name ILIKE '%' || $5 || '%')
        ORDER BY n.id {order}
        LIMIT $1
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(params.limit)
        .bind(cur_id)
        .bind(&params.filter_collection)
        .bind(&params.filter_contract_id)
        .bind(&params.filter_name)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(map_nft_item).collect())
}

/// `GET /v1/nfts/:contract_id/:token_id` — composite lookup.
///
/// Per task 0264 Phase 8a, the external NFT identity is
/// `(contract C-strkey, token_id)` rather than the internal `nfts.id i32`
/// surrogate PK. Joining `soroban_contracts sc` resolves the C-strkey to
/// the BIGINT FK that `nfts.contract_id` actually stores (ADR 0030); the
/// `UNIQUE (contract_id, token_id)` index on `nfts` (migration
/// `0005_tokens_nfts.sql`) is used to satisfy the equality predicate.
pub async fn fetch_by_composite(
    pool: &PgPool,
    contract_id: &str,
    token_id: &str,
) -> Result<Option<NftItem>, sqlx::Error> {
    let raw: Option<PgRow> = sqlx::query(
        r#"
        SELECT
            n.id,
            sc.contract_id        AS contract_id,
            n.token_id,
            n.collection_name,
            n.name,
            n.media_url,
            n.minted_at_ledger,
            own.account_id        AS owner_account,
            n.current_owner_ledger AS last_seen_ledger
        FROM nfts n
        JOIN      soroban_contracts sc  ON sc.id  = n.contract_id
        LEFT JOIN accounts          own ON own.id = n.current_owner_id
        WHERE sc.contract_id = $1
          AND n.token_id     = $2
        "#,
    )
    .bind(contract_id)
    .bind(token_id)
    .fetch_optional(pool)
    .await?;
    Ok(raw.as_ref().map(map_nft_item))
}

/// Resolve a `(contract_id, token_id)` composite to the internal
/// `nfts.id i32` surrogate. Returns `None` when the NFT doesn't exist —
/// disambiguates 404 from `200 + data: []` on the transfers endpoint
/// while also surfacing the surrogate that downstream queries
/// (`fetch_transfers`, cursor payloads) need.
pub async fn nft_exists_by_composite(
    pool: &PgPool,
    contract_id: &str,
    token_id: &str,
) -> Result<Option<i32>, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as(
        r#"
        SELECT n.id
        FROM nfts n
        JOIN soroban_contracts sc ON sc.id = n.contract_id
        WHERE sc.contract_id = $1
          AND n.token_id     = $2
        "#,
    )
    .bind(contract_id)
    .bind(token_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// `GET /v1/nfts/:id/transfers` — paginated ownership history.
pub async fn fetch_transfers(
    pool: &PgPool,
    nft_id: i32,
    cursor: Option<&NftTransferCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<NftTransferItem>, sqlx::Error> {
    let (cur_ts, cur_ledger, cur_order): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<i64>,
        Option<i16>,
    ) = match cursor {
        Some(c) => (
            Some(c.created_at),
            Some(c.ledger_sequence),
            Some(c.event_order),
        ),
        None => (None, None, None),
    };
    let (op, order) = direction_sql(direction);

    // Direction caveat: the LEAD window walks DESC to compute the
    // previous owner (oldest event below the current row). When fetching
    // Prev (ASC), the LEAD direction would invert — but the caller
    // reverses the resulting rows in `finalize_page` to restore DESC
    // presentation order. The window function still computes owners in
    // DESC order so that previous-owner semantics stay correct after the
    // outer reverse: we ALWAYS read the window in DESC, regardless of
    // fetch direction. This is achieved by keeping the `OVER (... ORDER
    // BY ... DESC)` hardcoded; only the outer ORDER BY + cursor
    // predicate swap.
    let sql = format!(
        r#"
        SELECT
            no.created_at,
            no.ledger_sequence,
            no.event_order,
            nft_event_type_name(no.event_type)  AS event_type_name,
            no.event_type                       AS event_type,
            LEAD(own.account_id) OVER (
                PARTITION BY no.nft_id
                ORDER BY no.created_at DESC,
                         no.ledger_sequence DESC,
                         no.event_order DESC
            )                                   AS from_account,
            own.account_id                      AS to_account,
            encode(t.hash, 'hex')               AS transaction_hash
        FROM nft_ownership no
        LEFT JOIN accounts     own ON own.id = no.owner_id
        JOIN      transactions t
               ON t.id         = no.transaction_id
              AND t.created_at = no.created_at
        WHERE no.nft_id = $1
          AND ($3::timestamptz IS NULL
               OR (no.created_at, no.ledger_sequence, no.event_order) {op} ($3, $4, $5))
        ORDER BY no.created_at {order}, no.ledger_sequence {order}, no.event_order {order}
        LIMIT $2
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(nft_id)
        .bind(limit)
        .bind(cur_ts)
        .bind(cur_ledger)
        .bind(cur_order)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| NftTransferItem {
            transaction_hash: r.get("transaction_hash"),
            ledger_sequence: r.get("ledger_sequence"),
            event_type_name: r.get("event_type_name"),
            event_type: r.get("event_type"),
            from_account: r.get("from_account"),
            to_account: r.get("to_account"),
            created_at: r.get("created_at"),
            event_order: r.get("event_order"),
        })
        .collect())
}
