//! Runtime details enrichment — per-request, fail-soft, in-process.
//!
//! Transport-specific submodules share a common shape: best-effort fetch,
//! merge into the DB-light slice, on failure surface the enrichment fields as
//! `null` and warn-log (no `enrichment_status` discriminator today — re-introduce
//! when a frontend needs to distinguish "fetch failed" from "no data published").
//!
//! - [`stellar_archive`] — S3 reread of public Stellar archive ledgers (ADR 0029).
//! - [`sep1`] — HTTP fetch of issuer stellar.toml files (task 0188).
//! - [`nft_metadata`] — Soroban RPC `token_uri()` + IPFS gateway for the
//!   NFT detail page (task 0195 §2d).

pub mod nft_token_uri;
pub mod sep1;
pub mod stellar_archive;

use nft_token_uri::NftTokenUriFetcher;
use sep1::Sep1Fetcher;
use stellar_archive::StellarArchiveFetcher;

/// Bundle of every runtime-enrichment fetcher held in `AppState`.
///
/// All inner fetchers are cheaply cloneable (`Arc`-backed). One field per
/// transport: a future submodule (e.g. `price_oracle`) would be added
/// here without touching the existing surface.
///
/// Field names match the source pipeline (same convention as `sep1`
/// / `stellar_archive`): `nft_token_uri` for the per-token
/// `token_uri()` Soroban RPC + IPFS gateway. Wire response fields
/// (`metadata`, `description`, `home_page`, …) are mapped at the
/// handler boundary, independent of fetcher names.
#[derive(Clone)]
pub struct RuntimeEnrichment {
    pub stellar_archive: StellarArchiveFetcher,
    pub sep1: Sep1Fetcher,
    pub nft_token_uri: NftTokenUriFetcher,
}
