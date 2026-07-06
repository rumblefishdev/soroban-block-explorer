//! Request and response DTOs for the NFT endpoints.
//! Wire shapes mirror canonical SQL `endpoint-queries/{15,16,17}_*.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// `filter[...]` query parameters for `GET /v1/nfts`.
///
/// `limit` / `cursor` are read by a sibling `Pagination<NftListCursor>`
/// extractor and are intentionally absent here.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// Exact match against `nfts.collection_name` (btree
    /// `idx_nfts_collection`). Trigram support is task 0132.
    #[serde(rename = "filter[collection]")]
    pub filter_collection: Option<String>,
    /// Contract C-StrKey; resolved to `soroban_contracts.id` server-side.
    #[serde(rename = "filter[contract_id]")]
    pub filter_contract_id: Option<String>,
    /// Substring match against `nfts.name` via the `idx_nfts_name_trgm`
    /// GIN index. SQL wraps the value in `%...%`; caller MUST NOT pass
    /// `%` / `_` literals.
    #[serde(rename = "filter[name]")]
    pub filter_name: Option<String>,
}

/// One NFT row. Same shape on `GET /v1/nfts` list rows and as the
/// flattened core of `GET /v1/nfts/:id` (which adds `metadata`). Pinned
/// to canonical SQL `15_get_nfts_list.sql` for the column projection.
///
/// The numeric surrogate `id` was dropped (task 0243 NFT slice): the
/// external NFT identity is the composite `(contract_id, token_id)` per
/// task 0264 Phase 8a, and ClickHouse — the production datastore — has no
/// surrogate on `nfts` at all (`ORDER BY (contract_id, token_id)`). This
/// mirrors the assets `:id`-surrogate drop (PR #175).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NftItem {
    /// Contract C-StrKey resolved via `soroban_contracts` join.
    pub contract_id: String,
    pub token_id: String,
    pub collection_name: Option<String>,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub minted_at_ledger: Option<i64>,
    /// Current owner G-StrKey, or `null` for burned NFTs (ADR 0037 §13).
    pub owner_account: Option<String>,
    /// Most recent ledger where ownership state changed
    /// (`nfts.current_owner_ledger`).
    pub last_seen_ledger: Option<i64>,
}

/// Detail response for `GET /v1/nfts/:id`. The `NftItem` fields are
/// flattened in (same shape as the list-endpoint row, see
/// `15_get_nfts_list.sql` for the on-the-wire columns), plus a
/// `metadata` field fetched at request time via
/// `runtime_enrichment::nft_token_uri` — full JSON blob from the
/// per-token `token_uri()` IPFS / HTTP URL (attributes, traits,
/// description, animation_url, etc.). `metadata` is always present
/// in the response, set to `null` when the fetch fails or the contract
/// returns a non-JSON `image/*` URI (fail-soft per ADR 0043).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NftDetailResponse {
    #[serde(flatten)]
    #[schema(inline)]
    pub item: NftItem,
    /// Off-chain JSON metadata fetched at request time. Always present
    /// on the wire — `null` indicates fetch failure / unsupported
    /// content-type, not field absence.
    #[schema(value_type = Object, nullable, required)]
    pub metadata: Option<serde_json::Value>,
}

/// One row of NFT transfer history. Shape pinned to canonical SQL
/// `17_get_nfts_transfers.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NftTransferItem {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    /// `mint | transfer | burn` — pre-decoded via `nft_event_type_name(...)`.
    pub event_type_name: Option<String>,
    /// Raw NftEventType discriminant (ADR 0031).
    pub event_type: i16,
    /// Previous-owner G-StrKey reconstructed via `LEAD(owner_id)` over the
    /// per-NFT ownership timeline (DESC window — older event sits at the
    /// FOLLOWING window position). `null` on the mint row only.
    ///
    /// Page boundaries are handled implicitly by the `limit + 1` peek
    /// fetch: the peek row participates in the window-function input, so
    /// the last *kept* row's `from_account` reads the peek's owner before
    /// `finalize_page` drops the peek. No client-side stitching needed.
    pub from_account: Option<String>,
    /// New owner G-StrKey. `null` on burn.
    pub to_account: Option<String>,
    pub created_at: DateTime<Utc>,
    pub event_order: i16,
}

/// Cursor payload for `GET /v1/nfts`. Replaces the old `NftIdCursor{id}`
/// (the SERIAL surrogate was dropped — see [`NftItem`]). The list orders
/// by `(minted_at_ledger DESC, contract_id DESC, token_id DESC)` — a total
/// keyset that maps to the CH `nfts` PK suffix `(contract_id, token_id)`
/// with `minted_at_ledger` (recency, "newest mint first") as the lead key.
/// `contract_id` here is the internal Int64 surrogate (opaque per ADR 0008,
/// a pure cursor tiebreak), NOT the wire C-StrKey. Datasource-agnostic: the
/// PG path orders on the same tuple so the cursor round-trips across a flag
/// flip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftListCursor {
    /// `ifNull(minted_at_ledger, 0)` — lead key (DESC).
    pub minted_at_ledger: i64,
    /// Internal contract surrogate (PK suffix tiebreak; opaque).
    pub contract_surrogate: i64,
    /// `token_id` PK suffix tiebreak.
    pub token_id: String,
}

/// Cursor payload for `GET /v1/nfts/:id/transfers`. The natural keyset
/// is the `nft_ownership` PK `(nft_id, created_at, ledger_sequence,
/// event_order)`; `nft_id` is a path parameter so only the trailing
/// three components live in the cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftTransferCursor {
    pub created_at: DateTime<Utc>,
    pub ledger_sequence: i64,
    pub event_order: i16,
}

// ---------------------------------------------------------------------------
// Internal query types (not serialized; produced/consumed by queries_ch and
// the handler). Relocated from the deleted PG queries.rs (task 0244).
// ---------------------------------------------------------------------------

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
/// (not on the wire). `queries_ch` returns this so the handler stays
/// backend-agnostic after the fetch (same shape as the assets `AssetRow`).
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
