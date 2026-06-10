//! Assets API module — list / detail / transactions sub-resource.
//!
//! `:id` is multi-form (contract StrKey / `CODE-ISSUER` composite / the
//! reserved `native` token); resolution lives in `handlers::parse_asset_id`.
//! All three reads (list / detail / transactions) dispatch PG (`sqlx`) or CH
//! (`clickhouse`) per `DataSource::for_module(Module::Assets)` (task 0243).
//! `/transactions` carries a datasource-tagged `TxListCursor`; the CH identity
//! seek over `operations_appearances` has a read-cost caveat (see
//! `queries_ch::fetch_transactions`) — operator smoke before the prod flag flip.

pub mod dto;
mod handlers;
mod queries;
mod queries_ch;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_assets))
        .routes(routes!(handlers::get_asset))
        .routes(routes!(handlers::list_asset_transactions))
}
