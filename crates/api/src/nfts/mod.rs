//! NFTs API module: list, detail, and transfer history.
//!
//! Wire shapes mirror canonical SQL `endpoint-queries-clickhouse/{15,16,17}_*.sql`
//! (task 0167). Pagination, error envelopes, cursor codec, and StrKey
//! validation come from `crate::common::*` (task 0043).

pub mod dto;
mod handlers;
/// `pub(crate)` for [`queries::NFT_VISIBLE`] — `search` reads `nfts` too and
/// must carry the same visibility predicate (task 0392).
pub(crate) mod queries;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Build the nfts sub-router (mounted under `/v1` in `main::app`).
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_nfts))
        .routes(routes!(handlers::get_nft))
        .routes(routes!(handlers::list_nft_transfers))
}
