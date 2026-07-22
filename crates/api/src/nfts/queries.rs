//! ClickHouse queries for the NFT endpoints (task 0243 — NFT slice).
//!
//! Returns the [`NftRow`] / [`NftItem`] / [`NftTransferItem`] shapes, so the
//! handler stays backend-agnostic after the fetch. CH is the sole datastore
//! (PG removed, task 0244).
//!
//! Schema notes worth calling out (all verified against
//! `crates/db-clickhouse/schema/init.sql`):
//!
//! - **`nfts.{name,media_url,collection_name}` are vestigial NULL on CH.**
//!   The live indexer re-writes a whole `nfts` row on every ownership change
//!   with metadata NULL (it owns the `current_owner_ledger` RMT version), so
//!   the real metadata lives in the `nft_enrichment` side table (task 0231).
//!   We collapse it with `argMax(_, version) GROUP BY (contract_id, token_id)`
//!   — NOT a `FINAL` join — so an un-merged RMT duplicate can never multiply
//!   the base rows (same idiom as `asset_enrichment`). **Without this join CH
//!   NFTs read NULL names despite the enrichment table being populated.**
//! - **No surrogate `id` on CH `nfts`** (`ORDER BY (contract_id, token_id)`).
//!   The wire `NftItem.id` was dropped (see `dto.rs`); the list cursor keys on
//!   `(minted_at_ledger, contract_id, token_id)`, the transfers timeline keys
//!   on `(contract_id, token_id)` directly (no `nft_id` indirection).
//! - **`nfts n FINAL`** collapses re-ingested ownership versions on the base
//!   RMT. Identity lookups (`accounts` / `soroban_contracts`) are never a
//!   whole-dimension JOIN — a hash JOIN reads the entire right table (~23M /
//!   ~25M rows), which a `WHERE id IN (lit)` bloom seek (`idx_acc_id`) avoids
//!   (task 0355 / fix c468c356).
//! - **`accounts` reverse lookup (owner_id → G-StrKey)** uses that bloom seek:
//!   the list / transfers paths via a page-scoped restricted CTE (`WHERE id IN
//!   (page ids) GROUP BY id`, no `FINAL`); the single-row detail resolves its
//!   one owner id in Rust via [`resolve_accounts`] (the shared `WHERE id IN`
//!   resolver), and echoes the contract StrKey straight from the request input.
//! - **`nullIf(...)`** maps a JOIN miss / sentinel to `None`. We do NOT use
//!   `SETTINGS join_use_nulls = 1` — `api_reader` runs `readonly = 1` and
//!   rejects per-query setting overrides.
//! - **No `created_at` column on CH.** Transfers recover it via a JOIN to
//!   `ledgers.closed_at` (`millis_to_utc`), and the cursor keys on
//!   `(ledger_sequence, event_order)` — NOT the lossy `closed_at` (fix
//!   c03c098c). `created_at` stays in the cursor payload for wire byte-parity
//!   with the PG cursor; the CH `WHERE` ignores it.
//! - **`nft_event_type_name()` is a PG SQL function** with no CH equivalent —
//!   mapped in Rust ([`nft_event_type_name`]).
//! - **Positional `clickhouse::Row` decode:** every `SELECT` column order MUST
//!   match its Row struct field order; a reorder silently decodes into the
//!   wrong field. The `CH_URL`-gated `decode_smoke` test is the only guard.
//!
//! ponytail: the list does a full `nft_enrichment` collapse + a non-PK
//! `minted_at_ledger` sort, i.e. a full `nfts` scan per page. Fine at the
//! current hot-set (~12.8k NFTs). If the NFT count grows ~100x, add a skip
//! index on `minted_at_ledger` + page-scope the enrichment collapse (or a
//! denormalized enriched-nfts projection) — not before (YAGNI).

use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::{millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};

use super::dto::{NftItem, NftListCursor, NftTransferCursor, NftTransferItem};

/// Resolved, validated `GET /v1/nfts` list params handed to `fetch_list`.
pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<NftListCursor>,
    pub filter_collection: Option<String>,
    pub filter_contract_id: Option<String>,
    /// Raw substring (no `%` / `_` from caller). SQL composes `%...%`.
    pub filter_name: Option<String>,
}

/// NFT list row: the wire [`NftItem`] fields plus the internal
/// `contract_surrogate` the composite cursor needs for its PK-suffix tiebreak
/// (not on the wire). Returned by `fetch_list` so the handler stays decoupled
/// from the CH-decode struct (same shape as the assets `AssetRow`).
pub struct NftRow {
    pub contract_id: String,
    pub token_id: String,
    pub collection_name: Option<String>,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub minted_at_ledger: Option<i64>,
    pub owner_account: Option<String>,
    pub last_seen_ledger: Option<i64>,
    /// Internal `soroban_contracts.id` (CH `Int64`) surrogate — cursor
    /// tiebreak only, never serialized.
    pub contract_surrogate: i64,
}

/// Contract-id set whose **current** verdict is `Nft` (`ContractType::Nft = 2`,
/// asserted in `tests/nft_visibility_guard.rs`).
///
/// Task 0392 — visibility is a read-time predicate, not a physical table split.
/// A `transfer` event is byte-identical for an NFT and a fungible token, so the
/// class is undecidable at ingest and can resolve *after* the row is written.
/// The indexer therefore writes every NFT-shaped row it cannot rule out, and a
/// row becomes visible the moment its contract's verdict resolves to `Nft` —
/// with nothing to promote, move, or drain. (This replaces the `nfts_pending`
/// quarantine, whose only drain was a human running `backfill-runner
/// nft-reclassify`.)
///
/// `FINAL` is load-bearing: `soroban_contracts` is a
/// `ReplacingMergeTree(wasm_uploaded_at_ledger)`, so a non-FINAL read can serve
/// a pre-upgrade verdict.
///
/// ponytail: measured on prod 2026-07-21 (131k contracts, 122 of them `Nft`).
/// The `/v1/nfts` list page goes 24 ms / 49k read rows → **42 ms / 239k** with
/// this predicate; the predicate alone is 23 ms / 211k with `FINAL` vs 9 ms /
/// 168k without. Kept `FINAL`: dropping it makes visibility mean "some version
/// said `Nft`", which is right only while nothing ever downgrades a verdict
/// (true today — `build_wasm_upgrade_rows` carries `contract_type` forward).
/// Trade that for the 14 ms only if these endpoints ever get hot.
///
/// **Every** read of `nfts` / `nft_ownership` must carry it;
/// `tests/nft_visibility_guard.rs` fails the build if one does not.
pub(crate) const NFT_VISIBLE: &str =
    "(SELECT id FROM soroban_contracts FINAL WHERE contract_type = 2)";

/// `nft_ownership.event_type` SMALLINT → canonical label, matching the PG
/// `nft_event_type_name` function. Discriminants confirmed from
/// `domain::NftEventType` (Mint=0, Transfer=1, Burn=2) and the PG SQL `CASE`
/// (no `ELSE` → NULL). `None` for an out-of-range code preserves the PG-NULL
/// wire shape (the DTO field is `Option<String>`), NOT a degrade label.
fn nft_event_type_name(event_type: i16) -> Option<String> {
    match event_type {
        0 => Some("mint"),
        1 => Some("transfer"),
        2 => Some("burn"),
        _ => None,
    }
    .map(str::to_string)
}

// ---------------------------------------------------------------------------
// List — GET /v1/nfts (canonical 15)
// ---------------------------------------------------------------------------

/// SELECT column order MUST match the projection in [`fetch_list`] (positional
/// decode). `contract_id` is a bare `String`: `sc` is an INNER JOIN to a
/// page-scoped CTE, so the column is non-Nullable (every NFT has a known
/// contract). `last_seen_ledger` is `nullIf(current_owner_ledger, 0)` →
/// `Option` (CH stores `0` for "unknown"; PG stored NULL — `nullIf` aligns them).
#[derive(Debug, Row, Deserialize)]
struct NftListChRow {
    contract_id: String,
    token_id: String,
    collection_name: Option<String>,
    name: Option<String>,
    media_url: Option<String>,
    minted_at_ledger: Option<i64>,
    owner_account: Option<String>,
    last_seen_ledger: Option<i64>,
    contract_surrogate: i64,
}

fn map_list_row(r: NftListChRow) -> NftRow {
    NftRow {
        contract_id: r.contract_id,
        token_id: r.token_id,
        collection_name: r.collection_name,
        name: r.name,
        media_url: r.media_url,
        minted_at_ledger: r.minted_at_ledger,
        owner_account: r.owner_account,
        last_seen_ledger: r.last_seen_ledger,
        contract_surrogate: r.contract_surrogate,
    }
}

/// `GET /v1/nfts` — paginated list, ordered `(minted_at_ledger, contract_id,
/// token_id)` DESC (lead = recency; tail = the `nfts` PK suffix). Same tuple as
/// the PG path so the opaque cursor round-trips across a flag flip.
pub async fn fetch_list(
    client: &clickhouse::Client,
    params: &ResolvedListParams,
    direction: Direction,
) -> Result<Vec<NftRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Untrusted free-text + the C-StrKey are `.bind()`-ed; i64 cursor surrogates
    // are bound too (no interpolation anywhere here). The keyset clause is
    // present ONLY on continuation pages — page 1 binds no NULL into the tuple,
    // sidestepping the clickhouse-rs 0.15 "None-into-tuple keyset returns 0
    // rows" defect.
    let contract_clause = if params.filter_contract_id.is_some() {
        " AND n.contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1)"
    } else {
        ""
    };
    // Filter must match the SERVED value: coalesce(ledger METADATA name,
    // enrichment collection), ledger-precedence. Branch 1 — contract's latest
    // METADATA name = ?. Branch 2 — enrichment collection = ?, but only for
    // contracts with NO ledger name (else coalesce would serve the ledger name,
    // not the enrichment one). Keeps filter[collection] consistent with the
    // collection_name the list displays.
    let collection_pred = if params.filter_collection.is_some() {
        " AND ( \
             n.contract_id IN (SELECT sc0.id FROM soroban_contracts sc0 WHERE sc0.contract_id IN \
                 (SELECT contract_id FROM soroban_contract_metadata GROUP BY contract_id HAVING argMax(name, version) = ?)) \
             OR (e.collection_name = ? AND n.contract_id NOT IN \
                 (SELECT sc1.id FROM soroban_contracts sc1 WHERE sc1.contract_id IN \
                     (SELECT contract_id FROM soroban_contract_metadata GROUP BY contract_id HAVING argMax(name, version) != ''))) \
         )"
    } else {
        ""
    };
    let name_pred = if params.filter_name.is_some() {
        " AND positionCaseInsensitive(ifNull(e.name, ''), ?) > 0"
    } else {
        ""
    };
    let keyset = if params.cursor.is_some() {
        format!(" AND (ifNull(n.minted_at_ledger, 0), n.contract_id, n.token_id) {op} (?, ?, ?)")
    } else {
        String::new()
    };

    let sql = format!(
        "WITH \
         enr AS ( \
             SELECT contract_id, token_id, \
                    argMax(name, version)            AS name, \
                    argMax(media_url, version)       AS media_url, \
                    argMax(collection_name, version) AS collection_name \
             FROM nft_enrichment \
             GROUP BY contract_id, token_id \
         ), \
         page AS ( \
             SELECT n.contract_id          AS contract_surrogate, \
                    n.token_id             AS token_id, \
                    n.minted_at_ledger     AS minted_at_ledger, \
                    n.current_owner_id     AS current_owner_id, \
                    n.current_owner_ledger AS current_owner_ledger, \
                    e.name                 AS e_name, \
                    e.media_url            AS e_media_url, \
                    e.collection_name      AS e_collection_name \
             FROM nfts n FINAL \
             LEFT JOIN enr e ON e.contract_id = n.contract_id AND e.token_id = n.token_id \
             WHERE n.contract_id IN {NFT_VISIBLE}{contract_clause}{collection_pred}{name_pred}{keyset} \
             ORDER BY ifNull(n.minted_at_ledger, 0) {order}, n.contract_id {order}, n.token_id {order} \
             LIMIT ? \
         ), \
         own AS ( \
             SELECT id, any(account_id) AS account_id \
             FROM accounts \
             WHERE id IN (SELECT current_owner_id FROM page WHERE current_owner_id IS NOT NULL) \
             GROUP BY id \
         ), \
         sc AS ( \
             SELECT id, any(contract_id) AS contract_id \
             FROM soroban_contracts \
             WHERE id IN (SELECT contract_surrogate FROM page) \
             GROUP BY id \
         ), \
         scm AS ( \
             SELECT contract_id, argMax(name, version) AS name \
             FROM soroban_contract_metadata \
             WHERE contract_id IN (SELECT contract_id FROM sc) \
             GROUP BY contract_id \
         ) \
         SELECT \
             sc.contract_id                    AS contract_id, \
             p.token_id                        AS token_id, \
             coalesce(nullIf(scm.name, ''), nullIf(p.e_collection_name, '')) AS collection_name, \
             nullIf(p.e_name, '')              AS name, \
             nullIf(p.e_media_url, '')         AS media_url, \
             p.minted_at_ledger                AS minted_at_ledger, \
             nullIf(own.account_id, '')        AS owner_account, \
             nullIf(p.current_owner_ledger, 0) AS last_seen_ledger, \
             p.contract_surrogate              AS contract_surrogate \
         FROM page p \
         INNER JOIN sc  ON sc.id  = p.contract_surrogate \
         LEFT JOIN  scm ON scm.contract_id = sc.contract_id \
         LEFT JOIN  own ON own.id = p.current_owner_id \
         ORDER BY ifNull(p.minted_at_ledger, 0) {order}, p.contract_surrogate {order}, p.token_id {order}"
    );

    let mut query = client.query(&sql);
    if let Some(c) = &params.filter_contract_id {
        query = query.bind(c);
    }
    if let Some(c) = &params.filter_collection {
        // Two placeholders in `collection_pred`: ledger-name match + enrichment
        // match (same filter value bound to both).
        query = query.bind(c).bind(c);
    }
    if let Some(n) = &params.filter_name {
        query = query.bind(n);
    }
    if let Some(c) = &params.cursor {
        query = query
            .bind(c.minted_at_ledger)
            .bind(c.contract_surrogate)
            .bind(&c.token_id);
    }
    // `params.limit` is the handler's `fetch_limit()` (already the peek +1).
    let rows = query.bind(params.limit).fetch_all::<NftListChRow>().await?;
    Ok(rows.into_iter().map(map_list_row).collect())
}

// ---------------------------------------------------------------------------
// Detail — GET /v1/nfts/{contract_id}/{token_id} (canonical 16)
// ---------------------------------------------------------------------------

/// SELECT column order MUST match [`fetch_by_composite`] (positional decode).
/// `current_owner_id` is the raw `Nullable(Int64)` surrogate (unowned NFT →
/// NULL); its G-StrKey is resolved in Rust via [`resolve_accounts`], not a
/// whole-`accounts` JOIN. The `contract_id` StrKey is the request input, echoed
/// back (no `soroban_contracts` JOIN).
#[derive(Debug, Row, Deserialize)]
struct NftChRow {
    current_owner_id: Option<i64>,
    token_id: String,
    collection_name: Option<String>,
    name: Option<String>,
    media_url: Option<String>,
    minted_at_ledger: Option<i64>,
    last_seen_ledger: Option<i64>,
}

/// `GET /v1/nfts/{contract_id}/{token_id}` — single-row composite lookup.
///
/// One `nfts` PK seek (`(contract_id, token_id)`, resolving the C-StrKey → Int64
/// surrogate in the `cid` CTE) plus a scoped `nft_enrichment` collapse. The
/// owner G-StrKey is resolved in Rust via [`resolve_accounts`] (a `WHERE id IN`
/// bloom seek on the single owner id) instead of a whole-`accounts` JOIN, and
/// the contract StrKey is echoed from the request input instead of a
/// whole-`soroban_contracts` JOIN — the two joins that turned this into a
/// ~25M-row dimension scan (task 0355; same swap as 0344/0345/0354).
pub async fn fetch_by_composite(
    client: &clickhouse::Client,
    contract_id: &str,
    token_id: &str,
) -> Result<Option<NftItem>, clickhouse::error::Error> {
    let sql = format!(
        "WITH cid AS ( \
                   SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1 \
               ) \
               SELECT \
                   n.current_owner_id                AS current_owner_id, \
                   n.token_id                        AS token_id, \
                   coalesce( \
                       nullIf((SELECT argMax(name, version) FROM soroban_contract_metadata WHERE contract_id = ?), ''), \
                       nullIf(ne.collection_name, '') \
                   )                                 AS collection_name, \
                   nullIf(ne.name, '')               AS name, \
                   nullIf(ne.media_url, '')          AS media_url, \
                   n.minted_at_ledger                AS minted_at_ledger, \
                   nullIf(n.current_owner_ledger, 0) AS last_seen_ledger \
               FROM nfts n FINAL \
               LEFT JOIN ( \
                   SELECT contract_id, token_id, \
                          argMax(name, version)            AS name, \
                          argMax(media_url, version)       AS media_url, \
                          argMax(collection_name, version) AS collection_name \
                   FROM nft_enrichment \
                   WHERE contract_id IN (SELECT id FROM cid) \
                   GROUP BY contract_id, token_id \
               ) ne ON ne.contract_id = n.contract_id AND ne.token_id = n.token_id \
               WHERE n.contract_id IN (SELECT id FROM cid) \
                 AND n.contract_id IN {NFT_VISIBLE} \
                 AND n.token_id = ? \
               LIMIT 1"
    );
    let Some(r) = client
        .query(&sql)
        .bind(contract_id)
        .bind(contract_id)
        .bind(token_id)
        .fetch_optional::<NftChRow>()
        .await?
    else {
        return Ok(None);
    };

    // Owner G-StrKey via a single-id `WHERE id IN` bloom seek (unowned → None).
    // `.filter(non-empty)` preserves the old `nullIf(own.account_id, '')` shape.
    let owner_account = match r.current_owner_id {
        Some(id) => resolve_accounts(client, vec![id])
            .await?
            .remove(&id)
            .filter(|s| !s.is_empty()),
        None => None,
    };

    Ok(Some(NftItem {
        contract_id: contract_id.to_string(),
        token_id: r.token_id,
        collection_name: r.collection_name,
        name: r.name,
        media_url: r.media_url,
        minted_at_ledger: r.minted_at_ledger,
        owner_account,
        last_seen_ledger: r.last_seen_ledger,
    }))
}

/// Existence probe for the transfers endpoint's 404-vs-empty disambiguation.
/// Returns `true` iff `(contract_id, token_id)` exists in `nfts` **and its
/// contract currently reads as `Nft`** ([`NFT_VISIBLE`]) — an unclassified
/// contract's row must 404 like it did when it sat in the quarantine table. No
/// `FINAL` on `nfts` (existence doesn't care which version), no enrichment /
/// owner resolution.
pub async fn nft_exists(
    client: &clickhouse::Client,
    contract_id: &str,
    token_id: &str,
) -> Result<bool, clickhouse::error::Error> {
    let sql = format!(
        "SELECT 1 \
               FROM nfts \
               WHERE contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1) \
                 AND contract_id IN {NFT_VISIBLE} \
                 AND token_id = ? \
               LIMIT 1"
    );
    let hit = client
        .query(&sql)
        .bind(contract_id)
        .bind(token_id)
        .fetch_optional::<u8>()
        .await?;
    Ok(hit.is_some())
}

// ---------------------------------------------------------------------------
// Transfers — GET /v1/nfts/{contract_id}/{token_id}/transfers (canonical 17)
// ---------------------------------------------------------------------------

/// SELECT column order MUST match [`fetch_transfers`] (positional decode).
/// `created_at_ms` reads `ledgers.closed_at` (`DateTime64(3)`) as raw i64
/// millis, converted with [`millis_to_utc`] — same as the assets tx path.
#[derive(Debug, Row, Deserialize)]
struct NftTransferChRow {
    transaction_hash: Option<String>,
    ledger_sequence: i64,
    event_type: i16,
    to_account: Option<String>,
    from_account: Option<String>,
    created_at_ms: i64,
    event_order: i16,
}

fn map_transfer_row(r: NftTransferChRow) -> NftTransferItem {
    NftTransferItem {
        transaction_hash: r.transaction_hash.unwrap_or_default(),
        ledger_sequence: r.ledger_sequence,
        event_type_name: nft_event_type_name(r.event_type),
        event_type: r.event_type,
        from_account: r.from_account,
        to_account: r.to_account,
        created_at: millis_to_utc(r.created_at_ms),
        event_order: r.event_order,
    }
}

/// `GET /v1/nfts/{contract_id}/{token_id}/transfers` — paginated ownership
/// history, newest first.
///
/// `nft_ownership` is `ORDER BY (contract_id, token_id, ledger_sequence,
/// event_order)`, so the `(contract_id, token_id)` predicate is the leading PK
/// prefix → one granule-pruned seek per page. `LIMIT 1 BY (ledger_sequence,
/// event_order)` collapses re-ingest duplicates (plain RMT, no version column)
/// BEFORE the `LEAD` window reconstructs `from_account` — a duplicate would
/// otherwise corrupt the window. The `txs` join is a `(ledger_sequence, id)`
/// tuple seek (transactions is keyed on `ledger_sequence`; a bare `id IN`
/// can't prune) and is `GROUP BY id`-deduped so it stays provably 1:1 (panel
/// review: both fixes guard the `LEAD` window).
pub async fn fetch_transfers(
    client: &clickhouse::Client,
    contract_id: &str,
    token_id: &str,
    cursor: Option<&NftTransferCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<NftTransferItem>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Keyset keys on (ledger_sequence, event_order) — NOT the lossy closed_at.
    // Present only on continuation pages (page 1 binds no NULL into the tuple).
    let cursor_clause = if cursor.is_some() {
        format!(" AND (no.ledger_sequence, no.event_order) {op} (?, ?)")
    } else {
        String::new()
    };

    // The from-owner window is HARDCODED DESC regardless of fetch direction
    // (the previous owner is the older event = the FOLLOWING row in DESC
    // order); only the page ORDER BY + cursor comparator swap on Prev, and
    // `finalize_page` reverses Prev rows for presentation — same contract as
    // the PG query (`17_get_nfts_transfers.sql`).
    //
    // CH has NO SQL-standard `LEAD()` — it is `leadInFrame()`, and its DEFAULT
    // frame (`RANGE … CURRENT ROW`) excludes the following row, so it would
    // always return NULL. The explicit `ROWS BETWEEN UNBOUNDED PRECEDING AND
    // UNBOUNDED FOLLOWING` frame makes the next row in-frame, matching PG
    // `LEAD`. (This errors only against a live CH — the `decode_smoke` test
    // covers it; an offline build cannot.)
    let sql = format!(
        "WITH \
         page AS ( \
             SELECT no.ledger_sequence AS ledger_sequence, \
                    no.event_order     AS event_order, \
                    no.event_type      AS event_type, \
                    no.owner_id        AS owner_id, \
                    no.transaction_id  AS transaction_id \
             FROM nft_ownership no \
             WHERE no.contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1) \
               AND no.contract_id IN {NFT_VISIBLE} \
               AND no.token_id = ?{cursor_clause} \
             ORDER BY no.ledger_sequence {order}, no.event_order {order} \
             LIMIT 1 BY no.ledger_sequence, no.event_order \
             LIMIT ? \
         ), \
         owners AS ( \
             SELECT id, any(account_id) AS account_id \
             FROM accounts \
             WHERE id IN (SELECT owner_id FROM page WHERE owner_id IS NOT NULL) \
             GROUP BY id \
         ), \
         txs AS ( \
             SELECT id, any(lower(hex(hash))) AS hash \
             FROM transactions \
             WHERE (ledger_sequence, id) IN (SELECT ledger_sequence, transaction_id FROM page) \
               AND intDiv(ledger_sequence, 500000) IN (SELECT DISTINCT intDiv(ledger_sequence, 500000) FROM page) \
             GROUP BY id \
         ), \
         led AS ( \
             SELECT sequence, any(closed_at) AS closed_at \
             FROM ledgers \
             WHERE sequence IN (SELECT ledger_sequence FROM page) \
             GROUP BY sequence \
         ) \
         SELECT \
             nullIf(txs.hash, '')   AS transaction_hash, \
             p.ledger_sequence      AS ledger_sequence, \
             p.event_type           AS event_type, \
             nullIf(own.account_id, '') AS to_account, \
             leadInFrame(nullIf(own.account_id, '')) OVER ( \
                 ORDER BY p.ledger_sequence DESC, p.event_order DESC \
                 ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING \
             )                      AS from_account, \
             led.closed_at          AS created_at_ms, \
             p.event_order          AS event_order \
         FROM page p \
         LEFT JOIN owners own ON own.id = p.owner_id \
         LEFT JOIN txs        ON txs.id = p.transaction_id \
         INNER JOIN led       ON led.sequence = p.ledger_sequence \
         ORDER BY p.ledger_sequence {order}, p.event_order {order}"
    );

    let mut query = client.query(&sql).bind(contract_id).bind(token_id);
    if let Some(c) = cursor {
        query = query.bind(c.ledger_sequence).bind(c.event_order);
    }
    let rows = query.bind(limit).fetch_all::<NftTransferChRow>().await?;
    Ok(rows.into_iter().map(map_transfer_row).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nft_event_type_name_matches_pg_function() {
        assert_eq!(nft_event_type_name(0).as_deref(), Some("mint"));
        assert_eq!(nft_event_type_name(1).as_deref(), Some("transfer"));
        assert_eq!(nft_event_type_name(2).as_deref(), Some("burn"));
        // Out-of-range → None, matching the PG CASE's NULL (no degrade label).
        assert_eq!(nft_event_type_name(3), None);
        assert_eq!(nft_event_type_name(-1), None);
    }
}

/// Live-CH decode smoke for the NFT read path. The curl `FORMAT` box smokes
/// do NOT exercise the clickhouse-rs RowBinary decoder, so a wire-type↔struct
/// mismatch (e.g. a Nullable column decoded into a non-Option field, or a
/// positional reorder) passes a curl check yet 500s the live endpoint. This
/// decodes rows a real CH produced for each NFT fetch fn.
///
/// **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green.
/// Run against a reachable CH (local replica or SSH tunnel):
///
/// ```text
/// CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
///   cargo test -p api --lib nfts::queries::decode_smoke -- --nocapture
/// ```
#[cfg(test)]
mod decode_smoke {
    use super::*;
    use crate::common::cursor::Direction;

    fn client() -> Option<clickhouse::Client> {
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

    /// Every NFT CH row struct must decode the rows a real CH emits.
    #[tokio::test]
    async fn nft_ch_rows_decode() {
        let Some(ch) = client() else {
            eprintln!("CH_URL unset — skipping NFT CH decode smoke");
            return;
        };

        // `list` returns rows on any populated CH → always exercises the
        // `NftListChRow` decode, and bootstraps a real (contract_id, token_id)
        // for the per-NFT fetches below.
        let params = ResolvedListParams {
            limit: 5,
            cursor: None,
            filter_collection: None,
            filter_contract_id: None,
            filter_name: None,
        };
        let list = fetch_list(&ch, &params, Direction::Next)
            .await
            .expect("NftListChRow must decode");

        let Some(first) = list.first() else {
            eprintln!("CH has no NFTs — list decode ok, skipping per-NFT smoke");
            return;
        };
        let (contract_id, token_id) = (first.contract_id.clone(), first.token_id.clone());

        nft_exists(&ch, &contract_id, &token_id)
            .await
            .expect("nft_exists must run");
        fetch_by_composite(&ch, &contract_id, &token_id)
            .await
            .expect("NftChRow (detail) must decode");
        fetch_transfers(&ch, &contract_id, &token_id, None, 5, Direction::Next)
            .await
            .expect("NftTransferChRow must decode");
    }

    /// Task 0392 — the visibility contract, exercised against real rows.
    ///
    /// `nfts` / `nft_ownership` physically hold rows for contracts that are not
    /// classified `Nft`; every endpoint must behave as if those rows are absent,
    /// and must start serving them the moment the verdict resolves — with no
    /// promotion step. Data-driven: both the visible and the hidden contract are
    /// discovered from whatever the target CH holds, so this runs against a prod
    /// replica as-is.
    ///
    /// The verdict-flip half writes a `soroban_contracts` row, so it runs ONLY
    /// against a localhost CH; elsewhere the read-only half still runs.
    #[tokio::test]
    async fn unclassified_contracts_rows_are_present_but_invisible() {
        let Some(ch) = client() else {
            eprintln!("CH_URL unset — skipping NFT visibility smoke");
            return;
        };

        // A contract whose rows exist in `nfts` but whose verdict is not `Nft`.
        let hidden: Vec<(String, String)> = ch
            .query(&format!(
                "SELECT any(sc.contract_id), any(n.token_id) FROM nfts n \
                 INNER JOIN (SELECT id, any(contract_id) AS contract_id FROM soroban_contracts \
                             GROUP BY id) sc ON sc.id = n.contract_id \
                 WHERE n.contract_id NOT IN {NFT_VISIBLE} \
                 GROUP BY n.contract_id LIMIT 1"
            ))
            .fetch_all::<(String, String)>()
            .await
            .expect("hidden probe");

        let Some((hidden_contract, hidden_token)) = hidden.into_iter().next() else {
            eprintln!("no unclassified NFT rows on this CH — nothing to assert");
            return;
        };

        // 1. Every endpoint must treat the row as absent.
        assert!(
            !nft_exists(&ch, &hidden_contract, &hidden_token)
                .await
                .expect("exists probe"),
            "unclassified contract must read as non-existent (404), not 200"
        );
        assert!(
            fetch_by_composite(&ch, &hidden_contract, &hidden_token)
                .await
                .expect("detail probe")
                .is_none(),
            "detail must not serve an unclassified contract's row"
        );
        assert!(
            fetch_transfers(
                &ch,
                &hidden_contract,
                &hidden_token,
                None,
                20,
                Direction::Next
            )
            .await
            .expect("transfers probe")
            .is_empty(),
            "transfer history must not leak for an unclassified contract"
        );

        // 2. The list must not contain it either — checked over a page deep
        //    enough to cover the whole hot set on a test replica.
        let params = ResolvedListParams {
            limit: 5_000,
            cursor: None,
            filter_collection: None,
            filter_contract_id: None,
            filter_name: None,
        };
        let listed = fetch_list(&ch, &params, Direction::Next)
            .await
            .expect("list must decode");
        assert!(
            !listed.iter().any(|r| r.contract_id == hidden_contract),
            "list leaked an unclassified contract"
        );

        // 3. Verdict resolves → the same rows become visible, with nothing moved.
        let is_local = std::env::var("CH_URL")
            .map(|u| u.contains("localhost") || u.contains("127.0.0.1"))
            .unwrap_or(false);
        if !is_local {
            eprintln!("CH_URL is not localhost — skipping the verdict-flip half (it writes)");
            return;
        }

        let before = ch
            // nft-visibility-guard: counts PHYSICAL rows on purpose — the whole
            // assertion is that this number does not change when visibility does.
            .query(
                "SELECT count() FROM nfts WHERE contract_id = \
                 (SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1)",
            )
            .bind(&hidden_contract)
            .fetch_one::<u64>()
            .await
            .expect("row count before");

        // Classify it: a new `soroban_contracts` row with a higher RMT version.
        ch.query(
            "INSERT INTO soroban_contracts \
             SELECT id, contract_id, wasm_hash, wasm_uploaded_at_ledger + 1, deployer_id, \
                    deployed_at_ledger, 2, is_sac \
             FROM soroban_contracts FINAL WHERE contract_id = ?",
        )
        .bind(&hidden_contract)
        .execute()
        .await
        .expect("verdict flip insert");

        assert!(
            nft_exists(&ch, &hidden_contract, &hidden_token)
                .await
                .expect("exists after flip"),
            "a contract classified Nft must surface its existing rows immediately"
        );
        assert!(
            fetch_by_composite(&ch, &hidden_contract, &hidden_token)
                .await
                .expect("detail after flip")
                .is_some(),
            "detail must serve the row once the verdict resolves"
        );

        let after = ch
            // nft-visibility-guard: counts PHYSICAL rows on purpose — the whole
            // assertion is that this number does not change when visibility does.
            .query(
                "SELECT count() FROM nfts WHERE contract_id = \
                 (SELECT id FROM soroban_contracts WHERE contract_id = ? LIMIT 1)",
            )
            .bind(&hidden_contract)
            .fetch_one::<u64>()
            .await
            .expect("row count after");
        assert_eq!(
            before, after,
            "visibility must come from the verdict alone — no row was copied, \
             promoted, or rewritten"
        );

        // Restore the original verdict so a re-run starts from the same state.
        ch.query(
            "INSERT INTO soroban_contracts \
             SELECT id, contract_id, wasm_hash, wasm_uploaded_at_ledger + 1, deployer_id, \
                    deployed_at_ledger, 1, is_sac \
             FROM soroban_contracts FINAL WHERE contract_id = ?",
        )
        .bind(&hidden_contract)
        .execute()
        .await
        .expect("verdict restore");
    }
}
