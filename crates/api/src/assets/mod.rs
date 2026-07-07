//! Assets API module — list / detail / transactions sub-resource.
//!
//! `:id` is multi-form (contract StrKey / `CODE-ISSUER` composite / the
//! reserved `native` token); resolution lives in `handlers::parse_asset_id`.
//! All three reads (list / detail / transactions) are served from ClickHouse
//! (`queries`); PG was retired (task 0244). `/transactions` carries a
//! `TxListCursor`; the CH identity seek over `operations_appearances` has a
//! read-cost caveat (see `queries::fetch_transactions`).

pub mod dto;
mod handlers;
mod queries;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_assets))
        .routes(routes!(handlers::get_asset))
        .routes(routes!(handlers::list_asset_transactions))
}
