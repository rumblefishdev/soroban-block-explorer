//! Search API module: `GET /v1/search`.
//!
//! Spec sources:
//!   * lore task 0053
//!   * `queries` (authoritative SQL — one narrow query per entity bucket,
//!     fired only when the classifier says it can match, `per_group_limit`
//!     cap)
//!   * `docs/architecture/backend/backend-overview.md §6.3 Search`
//!
//! No caching (per task 0053): variable `q` makes a TTL cache useless
//! and the per-bucket `LIMIT` keeps each query bounded.

mod classifier;
pub mod dto;
mod handlers;
mod queries;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Build the search sub-router (mounted under `/v1` in `main::app`).
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(handlers::get_search))
}
