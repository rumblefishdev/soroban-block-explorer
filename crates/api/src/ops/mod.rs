//! Operational endpoints (health checks, diagnostics).

use axum::Json;
use serde_json::{Value, json};

/// Liveness probe consumed by AWS Lambda health checks and smoke tests.
#[utoipa::path(
    get,
    path = "/health",
    tag = "ops",
    // Liveness is exempt from the auth gate (see auth::is_exempt); override the
    // global security requirement with an empty one so the spec stays accurate
    // and Swagger "Try it out" on /health doesn't demand a token (task 0287).
    security(()),
    responses(
        (status = 200, description = "Service is healthy", body = serde_json::Value),
    ),
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
