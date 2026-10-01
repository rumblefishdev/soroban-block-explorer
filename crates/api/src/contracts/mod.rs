//! Contracts API module: detail, interface, invocations, events.
//!
//! The SQL behind each endpoint is in `queries` (invocations in
//! `queries/list_invocations.rs`). Pagination, error envelopes, cursor codec,
//! and StrKey validation come from `crate::common::*` (task 0043). Contract metadata
//! is small and gets a 45 s per-Lambda cache (`cache::ContractMetadataCache`).

pub mod cache;
pub mod dto;
mod handlers;
mod queries;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Build the contracts sub-router (mounted under `/v1` in `main::app`).
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(handlers::list_contracts))
        .routes(routes!(handlers::get_contract))
        .routes(routes!(handlers::get_interface))
        .routes(routes!(handlers::get_decompiled))
        .routes(routes!(handlers::list_invocations))
        .routes(routes!(handlers::list_events))
}
