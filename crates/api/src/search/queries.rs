//! ClickHouse queries backing `GET /v1/search` (task 0318).
//!
//! Returns `Vec<(String, SearchHit)>` with per-bucket `LIMIT` semantics,
//! so the handler stays backend-agnostic after the fetch. This is the sole
//! read path — PG was retired in prod (ADR 0047) and removed (task 0244).
//!
//! # Design — classification-gated, concurrent, per-bucket
//!
//! The PG implementation is one UNION of six WHERE-gated CTEs; CH evaluates
//! every branch even when its input is NULL, so a hash query would still scan
//! `assets` / `nfts` / `soroban_contracts` for a substring that provably (or
//! pathologically) cannot match. Instead this module fires **only the buckets
//! the classifier proves can match**, concurrently (`tokio::try_join!`):
//!
//! | mode (classifier)        | buckets fired                                |
//! |--------------------------|----------------------------------------------|
//! | `hash_bytes` (hex / L)   | `transaction`, `pool`                         |
//! | `strkey_prefix` (G… / C…)| `account`, `contract` (prefix), `asset`, `nft`|
//! | plain text               | `contract` (name), `asset`, `nft`             |
//!
//! This is the minimal-rows-scanned shape (the common hash lookup touches two
//! point-seeks and nothing else) and stays faithful to the PG result set for
//! every realistic input. The one intentional divergence: in `hash_bytes` mode
//! we skip the small-table substring buckets — `asset_code` (≤12 chars) can
//! never contain a 64-hex / 56-char needle (provably empty), and a contract /
//! NFT *named* after a full transaction hash is not a real search intent.
//!
//! # CH-vs-PG divergences (all verified against
//! `crates/db-clickhouse/schema/init.sql` and the live CH read modules)
//!
//! - **Transaction lookup** takes the candidate ledgers off
//!   `transaction_hash_prefix_index` (8-byte hash prefix, PK point-seek)
//!   through the transaction page's own `lookup_hash_ledgers`.
//!   `successful` + `last_activity_at` are
//!   resolved (PG parity) via a partition-pruned `transactions` seek + a
//!   `ledgers` PK join — both single-row, so the cost is two point-seeks.
//! - **NFT name** lives in `nft_enrichment`, NOT `nfts.name` (vestigial NULL on
//!   CH — the live indexer rewrites whole `nfts` rows on every ownership change
//!   with metadata NULL; task 0231). A `nfts.name` predicate would
//!   silently match nothing. We collapse the enrichment with
//!   `argMax(_, version)` (never `FINAL`) exactly like [`crate::nfts::queries`].
//! - **Contract name** lives in `soroban_contract_metadata` (the contract's own
//!   `name()`), NOT the dead `soroban_contracts.name` (no writer since task 0297).
//!   Contract free-text therefore matches Soroban-native tokens by their
//!   on-chain name; SACs are excluded from that table by design.
//! - **Asset issuer / contract StrKey** are resolved without a full-table hash
//!   join: the issuer surrogate → G-StrKey is a bloom-pruned
//!   `accounts WHERE id IN (page ids)` key-seek (`idx_acc_id`), never
//!   `LEFT JOIN accounts` (the ~23M-row hash-side build that OOMs — CH Code 241,
//!   the 0317 trap). `soroban_contracts`
//!   (smaller) is joined in-statement, same as the live `/assets` list.
//! - **`nullIf(...)`** maps a sentinel / JOIN miss to `None`. We do NOT use
//!   `SETTINGS join_use_nulls = 1` — `api_reader` runs `readonly = 1` (RBAC
//!   `read_only`) and rejects per-query setting overrides.
//! - **Positional `clickhouse::Row` decode:** every `SELECT` column order MUST
//!   match its Row struct field order; a reorder silently decodes into the
//!   wrong field.

use std::collections::HashMap;

use clickhouse::Row;
use serde::Deserialize;

use super::classifier::Classified;
use super::dto::{EntityType, SearchHit};

mod assets;
mod pools;
mod transactions;
use assets::search_assets;
use pools::search_pools;
use transactions::search_transactions;

/// Which entity buckets the `?type=` filter admits. Parsed from the query
/// string (backend-agnostic); passed into `fetch_search` to gate the
/// per-entity CTE branches.
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

/// `asset_type` SMALLINT → canonical label, matching the PG `asset_family_name`
/// function (same mapping as [`crate::assets::queries`], NOT the
/// `AssetType`-XDR `asset_type_name` used by accounts). `None` for an
/// out-of-range code (the PG `CASE` returns NULL with no `ELSE`).
fn asset_family_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native"),
        1 => Some("classic_credit"),
        // 2 (`sac`) retired — ADR 0051 (mirrors `assets::queries::asset_type_name`).
        3 => Some("soroban"),
        _ => None,
    }
    .map(str::to_string)
}

/// Canonical `/assets/:id` routing token, mirroring `canonical_id` in
/// `assets/handlers.rs` and the PG `asset_hits` CTE: contract StrKey if present,
/// else `CODE-ISSUER` (both parts resolved), else `native` (gated on
/// `asset_type = 0`). A malformed row (code present, issuer unresolved) yields
/// `None` — an honest no-route, never a mis-route to the native XLM page.
fn asset_route_token(
    contract_strkey: Option<&str>,
    asset_code: Option<&str>,
    issuer: Option<&str>,
    asset_type: i16,
) -> Option<String> {
    contract_strkey
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| match (asset_code, issuer) {
            (Some(code), Some(iss)) if !code.is_empty() && !iss.is_empty() => {
                Some(format!("{code}-{iss}"))
            }
            _ => None,
        })
        .or_else(|| (asset_type == 0).then(|| "native".to_string()))
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Runs the broad search. Fires the buckets the
/// classifier proves can match (see module docs), concurrently, and returns the
/// rows partitioned by `entity_type` for the handler to group. The returned
/// tuple's `String` is the entity-type literal (the handler ignores it and uses
/// the typed `SearchHit::entity_type`); kept for shape-parity with the PG path.
pub async fn fetch_search(
    client: &clickhouse::Client,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    let (tx, contract, asset, account, nft, pool) = tokio::try_join!(
        search_transactions(client, classified, include),
        search_contracts(client, q, classified, include, per_group_limit),
        search_assets(client, q, classified, include, per_group_limit),
        search_accounts(client, classified, include, per_group_limit),
        search_nfts(client, q, classified, include, per_group_limit),
        search_pools(client, q, classified, include, per_group_limit),
    )?;

    // Canonical union order (irrelevant to grouping — the handler buckets by
    // `entity_type` — but keeps the flat list tidy / diff-stable).
    let mut out = Vec::with_capacity(
        tx.len() + contract.len() + asset.len() + account.len() + nft.len() + pool.len(),
    );
    out.extend(tx);
    out.extend(contract);
    out.extend(asset);
    out.extend(account);
    out.extend(nft);
    out.extend(pool);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Accounts — G-StrKey prefix → PK range scan
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct AccountRow {
    account_id: String,
    label: String,
}

/// Fires only when the classifier produced a StrKey prefix. `account_id` is the
/// `accounts` ORDER BY key, so `startsWith(account_id, prefix) ORDER BY
/// account_id LIMIT` is an **early-terminating** primary-key range read — it
/// stops after a few granules regardless of how broad the prefix is.
///
/// **No `FINAL`** — a `FINAL` over the prefix range must merge the WHOLE range
/// before `LIMIT` applies, so its cost scales with the prefix breadth (measured:
/// `GA` read 98k/246k rows locally → ~9M on the 23M-row prod table). The plain
/// read instead reads a constant ~4 granules (measured: 33k for the broadest
/// `G` prefix). Re-ingest duplicates of one `account_id` are contiguous in sort
/// order and collapsed in Rust (keep-first); `home_domain` is version-stable
/// enough for a dropdown label, and post-merge the table is ~1 row per account.
/// A `C…` prefix matches no account (cheap empty range), mirroring the PG gate.
async fn search_accounts(
    client: &clickhouse::Client,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.account {
        return Ok(Vec::new());
    }
    let Some(prefix) = classified.strkey_prefix.as_deref() else {
        return Ok(Vec::new());
    };

    // `per_group_limit` is a handler-validated 1..=50 (no injection surface).
    let sql = format!(
        "SELECT account_id AS account_id, ifNull(home_domain, '') AS label \
         FROM accounts \
         WHERE startsWith(account_id, ?) \
         ORDER BY account_id \
         LIMIT {per_group_limit}"
    );
    let rows = client
        .query(&sql)
        .bind(prefix)
        .fetch_all::<AccountRow>()
        .await?;

    // Collapse adjacent re-ingest duplicates of the same `account_id` (rows are
    // `account_id`-ordered, so dupes are contiguous).
    let mut out = Vec::with_capacity(rows.len());
    let mut last_id: Option<String> = None;
    for r in rows {
        if last_id.as_deref() == Some(r.account_id.as_str()) {
            continue;
        }
        last_id = Some(r.account_id.clone());
        out.push((
            "account".to_string(),
            SearchHit {
                entity_type: EntityType::Account,
                identifier: r.account_id,
                label: r.label,
                route_token: None,
                successful: None,
                last_activity_at: None,
                contract_id: None,
                token_id: None,
            },
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Contracts — C-StrKey prefix (PK range) OR free-text name (metadata)
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct ContractIdRow {
    contract_id: String,
}

#[derive(Debug, Row, Deserialize)]
struct ContractNameRow {
    contract_id: String,
    name: String,
}

/// Two sub-modes, mirroring the PG `contract_hits` CTE:
/// * StrKey prefix set → `startsWith(contract_id, prefix) ORDER BY contract_id
///   LIMIT` PK range on `soroban_contracts`. **No `FINAL`** (same reasoning as
///   `search_accounts`: a FINAL merge over the prefix range scales with breadth;
///   `contract_id` is the immutable identity so the read needs no version
///   collapse) — duplicates are collapsed in Rust. The display name is then
///   resolved for the ≤`limit` matched ids via a BOUNDED `soroban_contract_
///   metadata WHERE contract_id IN (...)` PK seek (the live name source —
///   `soroban_contracts.name` is dead), NOT a per-query aggregation of the whole
///   metadata table.
/// * plain text → substring over `soroban_contract_metadata.name` (the
///   `$3 IS NULL` branch). Matches Soroban-native tokens by their on-chain
///   name; SACs are excluded from that table by design.
///
/// In `hash_bytes` mode (neither sub-mode applies) the bucket is skipped — a
/// contract named after a full transaction hash is not a real search intent.
async fn search_contracts(
    client: &clickhouse::Client,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.contract {
        return Ok(Vec::new());
    }

    // Resolved (contract_id, label) pairs, in match order.
    let pairs: Vec<(String, String)> = if let Some(prefix) = classified.strkey_prefix.as_deref() {
        // Phase 1: early-terminating PK-range scan for the matching contract ids.
        let sql = format!(
            "SELECT contract_id AS contract_id \
             FROM soroban_contracts \
             WHERE startsWith(contract_id, ?) \
             ORDER BY contract_id \
             LIMIT {per_group_limit}"
        );
        let id_rows = client
            .query(&sql)
            .bind(prefix)
            .fetch_all::<ContractIdRow>()
            .await?;

        // Collapse adjacent re-ingest duplicates (rows are contract_id-ordered).
        let mut ids: Vec<String> = Vec::with_capacity(id_rows.len());
        for r in id_rows {
            if ids.last().map(String::as_str) != Some(r.contract_id.as_str()) {
                ids.push(r.contract_id);
            }
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Phase 2: bounded name resolution — `contract_id` is the metadata PK, so
        // `IN (...)` is a key-seek over only the matched ids. The user-derived
        // StrKeys are `.bind()`-ed as an array.
        let placeholders = vec!["?"; ids.len()].join(",");
        // `ifNull(name, '')`: `soroban_contract_metadata.name` is
        // `Nullable(String)`, so the read projects a Nullable column. The wire
        // type MUST be non-nullable to decode into `ContractNameRow.name: String`
        // — without `ifNull` a contract whose latest metadata name is NULL makes
        // the RowBinary decoder mismatch (500). Empty string ⇒ "no name".
        let name_sql = format!(
            "SELECT contract_id AS contract_id, ifNull(name, '') AS name \
             FROM soroban_contract_metadata FINAL \
             WHERE contract_id IN ({placeholders})"
        );
        let mut name_q = client.query(&name_sql);
        for id in &ids {
            name_q = name_q.bind(id);
        }
        let names: HashMap<String, String> = name_q
            .fetch_all::<ContractNameRow>()
            .await?
            .into_iter()
            .map(|r| (r.contract_id, r.name))
            .collect();

        ids.into_iter()
            .map(|id| {
                let label = names.get(&id).cloned().unwrap_or_default();
                (id, label)
            })
            .collect()
    } else if classified.hash_bytes.is_none() {
        // Plain-text name search over the on-chain metadata. The
        // `ifNull(..., '')` keeps the projected column non-nullable so it decodes
        // into `ContractNameRow.name: String` (the WHERE filter alone does not
        // change the column's Nullable wire type).
        let sql = format!(
            "SELECT contract_id AS contract_id, ifNull(name, '') AS name \
             FROM soroban_contract_metadata FINAL \
             WHERE positionCaseInsensitive(ifNull(name, ''), ?) > 0 \
             LIMIT {per_group_limit}"
        );
        client
            .query(&sql)
            .bind(q)
            .fetch_all::<ContractNameRow>()
            .await?
            .into_iter()
            .map(|r| (r.contract_id, r.name))
            .collect()
    } else {
        return Ok(Vec::new());
    };

    Ok(pairs
        .into_iter()
        .map(|(contract_id, label)| {
            (
                "contract".to_string(),
                SearchHit {
                    entity_type: EntityType::Contract,
                    identifier: contract_id,
                    label,
                    route_token: None,
                    successful: None,
                    last_activity_at: None,
                    contract_id: None,
                    token_id: None,
                },
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// NFTs — name substring (from nft_enrichment), composite-id routing
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct NftSearchRow {
    identifier: String,
    label: String,
    contract_strkey: String,
    token_id: String,
}

/// Fires whenever the query is NOT hash-shaped. The searchable `name` lives in
/// `nft_enrichment` (NOT vestigial `nfts.name`), collapsed with
/// `argMax(_, version)`. The page is bounded by `LIMIT`, then the contract
/// surrogate is resolved → C-StrKey via a page-scoped `soroban_contracts`
/// lookup — the same structure as the live `/nfts` list. The NFT routes on the
/// `(contract_id, token_id)` composite (ADR 0030), carried on the hit's
/// `contract_id` / `token_id` fields; `route_token` stays `None`.
async fn search_nfts(
    client: &clickhouse::Client,
    q: &str,
    classified: &Classified,
    include: &IncludeFlags,
    per_group_limit: i32,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.nft || classified.hash_bytes.is_some() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "WITH \
         enr AS ( \
             SELECT contract_id, token_id, \
                    argMax(name, version)            AS name, \
                    argMax(collection_name, version) AS collection_name \
             FROM nft_enrichment GROUP BY contract_id, token_id \
         ), \
         page AS ( \
             SELECT n.contract_id     AS contract_surrogate, \
                    n.token_id        AS token_id, \
                    e.name            AS e_name, \
                    e.collection_name AS e_collection_name \
             FROM nfts n FINAL \
             LEFT JOIN enr e ON e.contract_id = n.contract_id AND e.token_id = n.token_id \
             WHERE positionCaseInsensitive(ifNull(e.name, ''), ?) > 0 \
             LIMIT {per_group_limit} \
         ), \
         sc AS ( \
             SELECT id, any(contract_id) AS contract_id \
             FROM soroban_contracts \
             WHERE id IN (SELECT contract_surrogate FROM page) GROUP BY id \
         ), \
         scm AS ( \
             SELECT contract_id, name FROM soroban_contract_metadata FINAL \
             WHERE contract_id IN (SELECT contract_id FROM sc) \
         ) \
         SELECT \
             ifNull(p.e_name, '')            AS identifier, \
             ifNull(coalesce(nullIf(scm.name, ''), nullIf(p.e_collection_name, '')), '') AS label, \
             sc.contract_id                  AS contract_strkey, \
             p.token_id                      AS token_id \
         FROM page p INNER JOIN sc ON sc.id = p.contract_surrogate LEFT JOIN scm ON scm.contract_id = sc.contract_id"
    );
    let rows = client
        .query(&sql)
        .bind(q)
        .fetch_all::<NftSearchRow>()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                "nft".to_string(),
                SearchHit {
                    entity_type: EntityType::Nft,
                    identifier: r.identifier,
                    label: r.label,
                    route_token: None,
                    successful: None,
                    last_activity_at: None,
                    contract_id: Some(r.contract_strkey),
                    token_id: Some(r.token_id),
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod decode_smoke;
