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
//! One consumer: the `enrichment-worker` Lambda (and the backfill runner,
//! same code) — it stores the whole JSON and the raw `token_uri` in
//! `nft_enrichment.{metadata, token_uri}` and extracts `name`, `image` →
//! `media_url`, `collection` for the list. The API serves the stored JSON on
//! `GET /v1/nfts/:id` and makes no `token_uri` call (task 0655, amended
//! ADR 0043).
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
//! Same as the SEP-1 fetcher: in-process `moka` LRU 24h, capacity 1024.
//! `resolve()` returns `Result<Option<(String, Value)>, Arc<NftTokenUriError>>`
//! (the raw `token_uri` and its JSON); the worker classifies an `Err` as
//! transient or permanent for SQS retry (via [`errors::is_transient`]). `moka`'s
//! `try_get_with` only caches `Ok` values, so neither transient nor
//! permanent errors poison the slot — every failure call site keeps
//! its warn log + retry semantics intact, at the cost of re-running
//! the fetch on repeat traffic for a broken NFT. Self-healing on
//! flaky upstreams (IPFS gateway 4xx blip) is preferred over the
//! sub-ms cache-hit savings.
//!
//! ### Two `token_uri` response conventions (Content-Type branch)
//!
//! - `application/json` — OpenZeppelin / OpenSea standard. Parse →
//!   extract `name`, `image` (resolve `ipfs://` → HTTPS), `collection`.
//!   The whole JSON is stored for the detail page.
//! - `image/*` — direct-image convention (JamesBachini Soroban
//!   example). The URI itself is the image; the fetcher synthesises
//!   `{"image": "<url>"}` so the worker writes only `media_url` and
//!   `name` / `collection_name` legitimately fall through to the
//!   empty-string sentinel.

mod client;
pub mod errors;
mod inline_metadata;
mod validate_uri;

pub use client::NftTokenUriFetcher;
pub(crate) use client::resolve_ipfs_to_https;
