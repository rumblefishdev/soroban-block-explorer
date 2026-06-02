//! Re-export shim over [`enrichment_shared::nft_token_uri`].
//!
//! Per ADR 0043 detail-only carve-out and task 0195 §2d: the per-token
//! `token_uri()` JSON blob is fetched at request time on
//! `GET /v1/nfts/:id` rather than persisted. The `nfts.metadata` JSONB
//! column was dropped in migration
//! `20260507120000_drop_nfts_metadata.up.sql`; this shim is the
//! per-request type-2 path that supplies the `metadata` field in the
//! detail wire response.
//!
//! Module is source-named (matching `runtime_enrichment::sep1`,
//! `runtime_enrichment::stellar_archive`): the source pipeline is the
//! per-token `token_uri()` Soroban RPC + HTTP / IPFS gateway. The
//! response wire field is still called `metadata` — that is an API
//! contract concern, not a module-name concern.
//!
//! The same fetcher is reused by the `enrichment-worker` Lambda when
//! it writes the list-endpoint columns (`name`, `media_url`,
//! `collection_name`).

pub use enrichment_shared::nft_token_uri::NftTokenUriFetcher;
