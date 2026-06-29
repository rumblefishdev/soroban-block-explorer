//! Row structs for the CH writer — production schema (hybrid: surrogate
//! `id Int64` on the three high-cardinality FK hubs, natural / composite
//! keys elsewhere).
//!
//! One `#[derive(clickhouse::Row, serde::Serialize)]` struct per table
//! in `crates/db-clickhouse/schema/init.sql`. Column name + order
//! match `init.sql` byte-for-byte (RowBinary is positional —
//! mismatch silently corrupts every row written). Column-order
//! pinning tests in `persist/tests_cross.rs` guard against drift.
//!
//! ## Surrogate-id hubs (`accounts`, `soroban_contracts`, `transactions`)
//!
//! Each carries `id: i64` derived deterministically via
//! [`super::ids`] (`cityhash64(natural_key)`). FK columns in 10+
//! downstream tables are `Int64` referencing these `id`s. See `ids.rs`
//! module docs for the why-this-three-tables rationale.
//!
//! ## Natural / composite keys (everything else)
//!
//! `assets`, `nfts`, `liquidity_pools`, `lp_positions`,
//! `liquidity_pool_snapshots`, `operations_appearances`,
//! `transaction_participants`, `nft_ownership` — composite ORDER BY
//! over already-cheap-shape columns (FixedString(32) hashes,
//! low-cardinality codes, Int64 FK references).
//!
//! ## Type translation
//!
//! - `Int64` → `i64`
//! - `Int32` → `i32`
//! - `Int16` → `i16`
//! - `Bool` → `bool`
//! - `String` (incl. `LowCardinality(String)`) → `String`
//! - `Nullable(String)` (incl. LC-wrapped) → `Option<String>`
//! - `FixedString(32)` → `[u8; 32]`
//! - `Nullable(FixedString(32))` → `Option<[u8; 32]>`
//! - `Decimal128(7)` → `i128` (scaled by 10⁷, matches PG `NUMERIC(28,7)`)
//! - `Nullable(Decimal128(7))` → `Option<i128>`
//! - `DateTime64(3, 'UTC')` → `i64` ms since Unix epoch

use clickhouse::Row;
use serde::{Deserialize, Serialize};

/// `ledgers` — immutable lookup, MergeTree, partitioned.
#[derive(Debug, Clone, Row, Serialize)]
pub struct LedgerRow {
    pub sequence: i64,
    pub hash: [u8; 32],
    /// `DateTime64(3, 'UTC')` — milliseconds since Unix epoch.
    pub closed_at: i64,
    pub protocol_version: i32,
    pub transaction_count: i32,
    pub base_fee: i64,
}

/// `wasm_interface_metadata` — immutable lookup, MergeTree.
#[derive(Debug, Clone, Row, Serialize)]
pub struct WasmInterfaceMetadataRow {
    pub wasm_hash: [u8; 32],
    pub metadata: String,
}

/// `accounts` — state hub, RMT(last_seen_ledger). Surrogate `id` for
/// FK joins; ORDER BY natural key `account_id` for direct lookups.
#[derive(Debug, Clone, Row, Serialize)]
pub struct AccountRow {
    pub id: i64,
    pub account_id: String,
    pub first_seen_ledger: i64,
    pub last_seen_ledger: i64,
    pub sequence_number: i64,
    pub home_domain: Option<String>,
}

/// `assets` — state, plain RMT. Composite PK: identity 4-tuple.
/// Native XLM: asset_type=0, asset_code='', issuer_id=0, contract_id=0.
/// `total_supply`/`holder_count` are DEAD columns (lore-0293): the indexer
/// writes them `None`; the live value is served from the pre-computed
/// `asset_aggregates` table (refreshable MV). Kept for backward-compat; drop
/// deferred to a cleanup task (0310).
#[derive(Debug, Clone, Row, Serialize)]
pub struct AssetRow {
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub contract_id: i64,
    pub name: Option<String>,
    pub total_supply: Option<i128>,
    pub holder_count: Option<i32>,
    pub icon_url: Option<String>,
    /// lore-0331 surrogate (`ids::asset_id`) — single-column asset key for
    /// `balances.asset_id`. Column order MUST match `assets` schema (id last).
    pub id: i64,
}

/// `asset_enrichment` — off-chain SEP-1 enrichment side table (task 0231),
/// RMT(version). Written by the enrichment worker, NOT the indexer; keyed
/// byte-for-byte like `assets` and joined at read. `version` =
/// `DateTime64(3, 'UTC')` (ms since epoch); a higher version wins, so the
/// worker can refresh or CLEAR (re-insert NULL with a newer `version`).
#[derive(Debug, Clone, Row, Serialize)]
pub struct AssetEnrichmentRow {
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub contract_id: i64,
    pub icon_url: Option<String>,
    pub name: Option<String>,
    pub version: i64,
}

/// `account_balances_current` — state, RMT(last_updated_ledger).
/// Trustline removals emitted as `balance = 0` rows; reads filter
/// `WHERE balance > 0`.
#[derive(Debug, Clone, Row, Serialize)]
pub struct AccountBalanceRow {
    pub account_id: i64,
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub balance: i128,
    pub last_updated_ledger: i64,
}

/// `soroban_contracts` — state hub, RMT(wasm_uploaded_at_ledger).
/// Surrogate `id`; `wasm_uploaded_at_ledger = 0` is the stub sentinel
/// (Pass 2 stub-rowing for referenced-but-not-deployed contracts).
///
/// Also read back (not just written): the task-0320 live WASM-upgrade prefetch
/// (`persist::fetch_prior_contract_rows`) reads the prior row in full so
/// `build_wasm_upgrade_rows` can carry the identity columns forward — hence
/// `Deserialize`.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct SorobanContractRow {
    pub id: i64,
    pub contract_id: String,
    pub wasm_hash: Option<[u8; 32]>,
    pub wasm_uploaded_at_ledger: i64,
    pub deployer_id: Option<i64>,
    pub deployed_at_ledger: Option<i64>,
    pub contract_type: Option<i16>,
    pub is_sac: bool,
    pub name: Option<String>,
}

/// `soroban_contract_metadata` — on-chain Soroban token metadata
/// (name/symbol/decimals) from the instance-storage `Symbol("METADATA")`
/// struct. RMT(version); `version` = observed ledger (latest wins). Per
/// `contract_id`; SACs are excluded by the producer
/// (`xdr_parser::extract_contract_metadata_writes`). Separate table — never
/// columns on `soroban_contracts` — to dodge the RMT whole-row clobber across
/// that table's many writers (deploy / rebuild EXCHANGE / stubs / db-merge).
/// See task 0297.
#[derive(Debug, Clone, Row, Serialize)]
pub struct SorobanContractMetadataRow {
    pub contract_id: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u32>,
    pub version: i64,
}

/// `balances` — unified per-holder balance (task 0331, Option C). RMT(version);
/// `last_updated_ledger` = observed ledger; removed/zeroed → `amount = 0`.
/// `holder_id` = `cityhash64(holder StrKey)` (→ `addresses.id`); `asset_id` =
/// `ids::asset_id` (→ `assets.id`); `amount` raw `Int128` (scale by the asset's
/// decimals at read). Replaces `soroban_token_balances` + (after step 6) classic
/// `account_balances_current`.
#[derive(Debug, Clone, Row, Serialize)]
pub struct BalanceRow {
    pub holder_id: i64,
    pub asset_id: i64,
    pub amount: i128,
    pub last_updated_ledger: i64,
}

/// `addresses` — unified address dimension (task 0331). One row per `ScAddress`;
/// resolves `balances.holder_id` → StrKey + kind. `id = cityhash64(strkey)`.
#[derive(Debug, Clone, Row, Serialize)]
pub struct AddressRow {
    pub id: i64,
    pub strkey: String,
    pub kind: String,
}

/// `soroban_token_supply` — authoritative per-token `total_supply` from the
/// instance `Symbol("TotalSupply")` i128 key (task 0331 step 7). RMT(version);
/// `asset_id` = `ids::asset_id` (→ `assets.id`). Absent for tokens that don't
/// store the key → the assets read falls back to `balance_aggregates`.
#[derive(Debug, Clone, Row, Serialize)]
pub struct SorobanTokenSupplyRow {
    pub asset_id: i64,
    pub total_supply: i128,
    pub last_updated_ledger: i64,
}

/// `nfts` — state, RMT(current_owner_ledger). Composite PK
/// = (contract_id, token_id). No surrogate id.
#[derive(Debug, Clone, Row, Serialize)]
pub struct NftRow {
    pub contract_id: i64,
    pub token_id: String,
    pub collection_name: Option<String>,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub minted_at_ledger: Option<i64>,
    pub current_owner_id: Option<i64>,
    pub current_owner_ledger: i64,
}

/// `nfts_pending` — task 0217 quarantine for NFT-candidate rows whose
/// contract is still `Other`/NULL-classified. Same shape as
/// [`NftRow`]; routed via `stage::prepare` based on the per-contract
/// `wasm_classification` verdict. Promoted to hot `nfts` via the
/// post-backfill drain runbook
/// (`docs/runbooks/0217_nfts_pending_migration_and_drain.md`) — CH
/// has no per-row UPDATE / `WHERE NOT EXISTS` equivalent to PG's
/// in-tx `promote_pending_nfts_to_hot` step.
#[derive(Debug, Clone, Row, Serialize)]
pub struct NftPendingRow {
    pub contract_id: i64,
    pub token_id: String,
    pub collection_name: Option<String>,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub minted_at_ledger: Option<i64>,
    pub current_owner_id: Option<i64>,
    pub current_owner_ledger: i64,
}

/// `nft_enrichment` — off-chain `token_uri` enrichment side table (task 0231),
/// RMT(version). Same rationale as [`AssetEnrichmentRow`]: written by the
/// enrichment worker (not the indexer), keyed like `nfts`, joined at read;
/// `version` is the worker's own clock (`DateTime64(3, 'UTC')`, ms),
/// independent of the `nfts` ownership clock.
#[derive(Debug, Clone, Row, Serialize)]
pub struct NftEnrichmentRow {
    pub contract_id: i64,
    pub token_id: String,
    pub name: Option<String>,
    pub media_url: Option<String>,
    pub collection_name: Option<String>,
    pub version: i64,
}

/// `liquidity_pools` — state, RMT(last_updated_ledger). PK = pool_id.
/// `created_at_ledger` removed (derive read-time from snapshots).
#[derive(Debug, Clone, Row, Serialize)]
pub struct LiquidityPoolRow {
    pub pool_id: [u8; 32],
    pub asset_a_type: i16,
    pub asset_a_code: String,
    pub asset_a_issuer_id: i64,
    pub asset_b_type: i16,
    pub asset_b_code: String,
    pub asset_b_issuer_id: i64,
    pub fee_bps: i32,
    pub last_updated_ledger: i64,
}

/// `lp_positions` — state, RMT(last_updated_ledger).
#[derive(Debug, Clone, Row, Serialize)]
pub struct LpPositionRow {
    pub pool_id: [u8; 32],
    pub account_id: i64,
    pub shares: i128,
    pub first_deposit_ledger: i64,
    pub last_updated_ledger: i64,
}

/// `transactions` — append-only fact hub, surrogate `id`,
/// ORDER BY (ledger_sequence, application_order).
#[derive(Debug, Clone, Row, Serialize)]
pub struct TransactionRow {
    pub id: i64,
    pub hash: [u8; 32],
    pub ledger_sequence: i64,
    pub application_order: i16,
    pub source_id: i64,
    pub fee_charged: i64,
    pub inner_tx_hash: Option<[u8; 32]>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub parse_error: bool,
}

/// `transaction_hash_index` — fact, backs `transaction_hash_dict`.
#[derive(Debug, Clone, Row, Serialize)]
pub struct TransactionHashIndexRow {
    pub hash: [u8; 32],
    pub ledger_sequence: i64,
}

/// `operations_appearances` — fact, no surrogate id. ORDER BY
/// (ledger_sequence, transaction_id, application_order).
#[derive(Debug, Clone, Row, Serialize)]
pub struct OperationAppearanceRow {
    pub transaction_id: i64,
    pub application_order: i16,
    /// `type` is a Rust keyword — serde rename keeps the CH column
    /// match clean.
    #[serde(rename = "type")]
    pub op_type: i16,
    pub source_id: Option<i64>,
    pub destination_id: Option<i64>,
    pub contract_id: Option<i64>,
    pub asset_code: String,
    pub asset_issuer_id: Option<i64>,
    /// Crossed liquidity pools, sorted + deduped (canonical order — see the
    /// stage fold). Empty = no pool involvement; `[]` replaces the legacy
    /// scalar NULL (task 0261/0268).
    pub pool_ids: Vec<[u8; 32]>,
    pub amount: i64,
    pub ledger_sequence: i64,
}

/// `transaction_participants` — fact.
#[derive(Debug, Clone, Row, Serialize)]
pub struct TransactionParticipantRow {
    pub account_id: i64,
    pub ledger_sequence: i64,
    pub transaction_id: i64,
}

/// `soroban_events` — fact, full-content per-event row (ADR 0044
/// §4a unfold). `signature` is the lifted first-topic Symbol.
#[derive(Debug, Clone, Row, Serialize)]
pub struct SorobanEventRow {
    pub contract_id: i64,
    pub transaction_id: i64,
    pub ledger_sequence: i64,
    pub event_index: i16,
    pub event_type: i16,
    pub signature: Option<String>,
    pub topics_xdr: String,
    pub data_xdr: String,
}

/// `soroban_invocations_appearances` — fact (ADR 0034 fold).
#[derive(Debug, Clone, Row, Serialize)]
pub struct SorobanInvocationAppearanceRow {
    pub contract_id: i64,
    pub transaction_id: i64,
    pub ledger_sequence: i64,
    pub caller_id: Option<i64>,
    pub caller_contract_id: Option<i64>,
    pub amount: i32,
}

/// `nft_ownership` — fact, no surrogate. ORDER BY
/// (contract_id, token_id, ledger_sequence, event_order).
#[derive(Debug, Clone, Row, Serialize)]
pub struct NftOwnershipRow {
    pub contract_id: i64,
    pub token_id: String,
    pub ledger_sequence: i64,
    pub event_order: i16,
    pub transaction_id: i64,
    pub owner_id: Option<i64>,
    pub event_type: i16,
}

/// `nft_ownership_pending` — task 0217 quarantine companion to
/// [`NftOwnershipRow`]. Same row shape + same partitioning as the hot
/// `nft_ownership` table so promotion (`INSERT … SELECT FROM
/// nft_ownership_pending`) is a clean part copy. Routed by the same
/// per-contract classifier verdict as [`NftPendingRow`].
#[derive(Debug, Clone, Row, Serialize)]
pub struct NftOwnershipPendingRow {
    pub contract_id: i64,
    pub token_id: String,
    pub ledger_sequence: i64,
    pub event_order: i16,
    pub transaction_id: i64,
    pub owner_id: Option<i64>,
    pub event_type: i16,
}

/// `liquidity_pool_snapshots` — fact, no surrogate. ORDER BY
/// (pool_id, ledger_sequence).
#[derive(Debug, Clone, Row, Serialize)]
pub struct LiquidityPoolSnapshotRow {
    pub pool_id: [u8; 32],
    pub ledger_sequence: i64,
    pub reserve_a: i128,
    pub reserve_b: i128,
    pub total_shares: i128,
    pub tvl: Option<i128>,
    pub volume: Option<i128>,
    pub fee_revenue: Option<i128>,
    /// Gross trade volume in asset-A units per (pool, ledger), from
    /// path-payment/offer claim atoms. NULL until the 0266 backfill / 0247
    /// wiring writes it (live ingest leaves it NULL today). Task 0261/0268.
    pub gross_volume_a: Option<i128>,
}
