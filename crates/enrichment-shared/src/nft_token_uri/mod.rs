//! `token_uri()` fetcher: per-NFT Soroban RPC + HTTP / IPFS gateway → JSON metadata file.
//!
//! Per task 0195 §2d. **Source naming rationale:** the source is the
//! string returned by the contract's `token_uri(token_id)` view
//! function (OpenZeppelin / OpenSea / ERC-721 metadata-extension
//! convention, copied by Soroban NFT contracts). The output is the
//! JSON file at that URL. Module is named after the *source* (token URI
//! pipeline), not after the dropped `nfts.metadata` schema column nor
//! after the api response field — both of which would be ambiguous.
//!
//! Two consumers:
//!
//! - `enrichment-worker` Lambda (type-1 SQS-driven, `EnrichmentMessage::NftTokenUri`)
//!   — extracts `name`, `image` → `media_url`, `collection` and writes the
//!   three list-endpoint columns `nfts.{name, media_url, collection_name}`
//!   to the DB. Those three are NOT derivable from the processed ledger
//!   alone — the standard NFT pattern leaves them in the off-chain JSON
//!   file referenced by `token_uri()`.
//! - api crate `runtime_enrichment::nft_token_uri` (type-2 per-request,
//!   thin shim over this module) — surfaces the full JSON blob on
//!   `GET /v1/nfts/:id` for the detail-page attribute table (wire
//!   field `metadata`). The blob is NOT persisted; the `nfts.metadata`
//!   JSONB column was dropped in migration
//!   `20260507120000_drop_nfts_metadata.up.sql` per ADR 0043.
//!
//! ### Source vs SEP-1 (assets)
//!
//! Different mechanism — do not conflate:
//!
//! - **SEP-1** is per-issuer: `accounts.home_domain` (on-chain) →
//!   `https://{home_domain}/.well-known/stellar.toml` → one TOML file
//!   covers every asset that issuer publishes. Cheap, ~KB, no IPFS.
//! - **`token_uri`** is per-token: Soroban RPC `simulateTransaction`
//!   invoking `token_uri(token_id)` → string URL → HTTP fetch (IPFS
//!   gateway resolve when the URL is `ipfs://`) → JSON. A 1000-token
//!   collection costs 1000 fetches.
//!
//! ### Cache + fail-soft semantics
//!
//! Same as the SEP-1 fetcher: in-process `moka` LRU 24h, errors mapped
//! to `None` at the call site, no caching of error results so a
//! transient failure on a cold key does not poison the slot. The api
//! never 5xx's because of an enrichment failure.
//!
//! ### Two `token_uri` response conventions (Content-Type branch)
//!
//! - `application/json` — OpenZeppelin / OpenSea standard. Parse →
//!   extract `name`, `image` (resolve `ipfs://` → HTTPS), `collection`.
//!   Surface the rest as the JSON blob to the api detail handler.
//! - `image/*` — direct-image convention (JamesBachini Soroban
//!   example). The URI itself is the image; `name` and
//!   `collection_name` get permanent-fail sentinels at write time.
//!
//! ### STUB STATUS
//!
//! The current revision is a stub. The Soroban RPC piece (build the
//! `simulateTransaction` envelope for `token_uri(token_id)`, decode
//! the SCV result) requires an in-house JSON-RPC + XDR-builder crate
//! that does not yet live in this workspace. The `NftTokenUriFetcher`
//! surface is finalised so consumers can compile and route through it
//! today; `resolve()` returns `None` (warn-logged) until the RPC client
//! lands. See task 0195 §2d for the full plan.

mod client;
pub mod errors;

pub use client::NftTokenUriFetcher;
