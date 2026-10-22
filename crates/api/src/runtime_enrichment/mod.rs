//! Runtime details enrichment — per-request, fail-soft, in-process.
//!
//! Transport-specific submodules share a common shape: best-effort fetch,
//! merge into the DB-light slice, on failure surface the enrichment fields as
//! `null` and warn-log (no `enrichment_status` discriminator today — re-introduce
//! when a frontend needs to distinguish "fetch failed" from "no data published").
//!
//! - [`stellar_archive`] — S3 reread of public Stellar archive ledgers (ADR 0029).
//! - [`sep1`] — HTTP fetch of issuer stellar.toml files (task 0188).
//! - [`source_verification`] — a verifier aggregator's Query API and the
//!   source archives it points to, for the contract page (task 0639).

pub mod account_sponsors;
pub mod rpc_pool;
pub mod sep1;
pub mod source_verification;
pub mod stellar_archive;
pub mod wasm_code;

use account_sponsors::AccountSponsorsFetcher;
use sep1::Sep1Fetcher;
use source_verification::SourceVerificationFetcher;
use stellar_archive::StellarArchiveFetcher;
use wasm_code::WasmCodeFetcher;

/// Bundle of every runtime-enrichment fetcher held in `AppState`.
///
/// All inner fetchers are cheaply cloneable (`Arc`-backed). One field per
/// transport: a future submodule (e.g. `price_oracle`) would be added
/// here without touching the existing surface.
///
/// Field names match the source pipeline (`sep1`, `stellar_archive`). Wire
/// response fields (`description`, `home_page`, …) are mapped at the handler
/// boundary, independent of fetcher names.
#[derive(Clone)]
pub struct RuntimeEnrichment {
    pub stellar_archive: StellarArchiveFetcher,
    pub sep1: Sep1Fetcher,
    pub wasm_code: WasmCodeFetcher,
    pub account_sponsors: AccountSponsorsFetcher,
    /// `None` on a network without a verifier service (`SOURCE_VERIFICATION_API_URL` unset).
    pub source_verification: Option<SourceVerificationFetcher>,
}
