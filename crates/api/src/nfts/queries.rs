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

use crate::common::cursor::{Direction, keyset_sql_desc};

use super::dto::{NftItem, NftListCursor, NftTransferCursor, NftTransferItem};

pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<NftListCursor>,
    pub filter_collection: Option<String>,
    pub filter_contract_id: Option<String>,
    /// Raw substring (no `%` / `_` from caller). SQL composes `%...%`.
    pub filter_name: Option<String>,
}

/// Datasource-agnostic NFT list row: the wire [`NftItem`] fields plus the
/// internal `contract_surrogate` the composite cursor needs for its PK-suffix
/// tiebreak (not on the wire). Both the PG path here and `queries_ch` return
/// this so the handler stays backend-agnostic after the fetch (same shape as
/// the assets `AssetRow`).
pub struct NftRow {
    pub contract_id: String,
    pub token_id: String,
    pub collection_name: Option<String>,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub minted_at_ledger: Option<i64>,
    pub owner_account: Option<String>,
    pub last_seen_ledger: Option<i64>,
    /// Internal `soroban_contracts.id` (PG `BIGINT` / CH `Int64`) surrogate —
    /// cursor tiebreak only, never serialized.
    pub contract_surrogate: i64,
}

fn map_nft_row(r: &PgRow) -> NftRow {
    NftRow {
        contract_id: r.get("contract_id"),
        token_id: r.get("token_id"),
        collection_name: r.get("collection_name"),
        name: r.get("name"),
        media_url: r.get("media_url"),
        minted_at_ledger: r.get("minted_at_ledger"),
        owner_account: r.get("owner_account"),
        last_seen_ledger: r.get("last_seen_ledger"),
        contract_surrogate: r.get("contract_surrogate"),
    }
}

fn map_nft_item(r: &PgRow) -> NftItem {
    NftItem {
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
/// Ordered by `(minted_at_ledger DESC, contract_id DESC, token_id DESC)` —
/// a total keyset matching the CH `nfts` PK suffix `(contract_id, token_id)`
/// with `minted_at_ledger` (recency) as the lead key. The old `id DESC`
/// order is gone with the surrogate (task 0243 NFT slice); this PG path
/// orders on the same tuple as `queries_ch::fetch_list` so the opaque
/// cursor round-trips across a datasource flip.
///
/// The contract-id resolve uses a CTE so it runs once even when the
/// planner materialises `idx_nfts_collection` / `idx_nfts_name_trgm`.
///
/// `filter_name` is wrapped in `'%' || $4 || '%'` for the trigram match.
/// We do NOT add an `ESCAPE` clause here: the upstream handler rejects
/// values containing literal `%` / `_` with a 400 envelope (mirrors the
/// `assets` handler convention). The value is always bound, never
/// concatenated, so the worst case of a bypass is a wider trigram match,
/// not SQL injection.
pub async fn fetch_list(
    pool: &PgPool,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<NftRow>, sqlx::Error> {
    let (cur_minted, cur_contract, cur_token): (Option<i64>, Option<i64>, Option<String>) =
        match &params.cursor {
            Some(c) => (
                Some(c.minted_at_ledger),
                Some(c.contract_surrogate),
                Some(c.token_id.clone()),
            ),
            None => (None, None, None),
        };
    let (op, order) = keyset_sql_desc(direction);

    // Static query plan per direction. SQL fragments `{op}` and `{order}`
    // are hardcoded literals (`<`/`>`, `DESC`/`ASC`) — no injection risk.
    let sql = format!(
        r#"
        WITH ct AS (
            SELECT id
            FROM soroban_contracts
            WHERE $3::varchar IS NOT NULL
              AND contract_id = $3
        )
        SELECT
            sc.contract_id        AS contract_id,
            n.token_id,
            n.collection_name,
            n.name,
            n.media_url,
            n.minted_at_ledger,
            own.account_id        AS owner_account,
            n.current_owner_ledger AS last_seen_ledger,
            n.contract_id         AS contract_surrogate
        FROM nfts n
        JOIN      soroban_contracts sc  ON sc.id = n.contract_id
        LEFT JOIN accounts          own ON own.id = n.current_owner_id
        WHERE
            ($2::varchar IS NULL OR n.collection_name = $2)
            AND ($3::varchar IS NULL OR n.contract_id = (SELECT id FROM ct))
            AND ($4::text    IS NULL OR n.name ILIKE '%' || $4 || '%')
            AND ($5::bigint  IS NULL
                 OR (COALESCE(n.minted_at_ledger, 0), n.contract_id, n.token_id) {op} ($5, $6, $7))
        ORDER BY COALESCE(n.minted_at_ledger, 0) {order}, n.contract_id {order}, n.token_id {order}
        LIMIT $1
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(params.limit)
        .bind(&params.filter_collection)
        .bind(&params.filter_contract_id)
        .bind(&params.filter_name)
        .bind(cur_minted)
        .bind(cur_contract)
        .bind(cur_token)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(map_nft_row).collect())
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
    let (op, order) = keyset_sql_desc(direction);

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
