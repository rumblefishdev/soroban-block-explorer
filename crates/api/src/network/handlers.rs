//! Axum handler for `GET /v1/network/stats`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::common::cache_control;
use crate::common::conditional;
use crate::common::errors;
use crate::common::head;
use crate::openapi::schemas::ErrorEnvelope;
use crate::state::AppState;

use super::dto::NetworkStats;
use super::queries;

/// Get top-level chain overview stats.
///
/// Reads the single-statement network-stats query (latest ledger row +
/// `ledgers` 60s aggregate for TPS + deduped counts of accounts /
/// contracts) and caches the assembled response **keyed on the chain head**
/// (`latest_ledger_sequence`) in process memory — see `network/cache.rs`.
/// See the task 0045 spec and `network::queries::fetch_stats` for the full
/// data-source mapping.
///
/// Per request we first read the head cheaply (`crate::common::head` —
/// `SELECT max(sequence)`, a primary-key probe) and look up the cache
/// under it: an unchanged head is a HIT (the new ledger has not landed),
/// an advanced head is a MISS that recomputes once. So a new ledger is
/// visible on the **first** request after it is written — there is no
/// up-to-TTL window serving the previous head (task 0291).
///
/// Concurrent cold-cache requests deduplicate via
/// `moka::future::Cache::try_get_with` — the first task runs the
/// async DB query and the rest wait on its result instead of fanning
/// out N round-trips.
#[utoipa::path(
    get,
    path = "/network/stats",
    tag = "network",
    responses(
        (status = 200, description = "Chain overview stats", body = NetworkStats),
        (status = 304, description = "Not Modified — `If-None-Match` matched the current chain head"),
        (status = 500, description = "Database error",       body = ErrorEnvelope),
    ),
)]
pub async fn get_network_stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // Cheap head read gates the cache: the head is the cache key, so a new
    // ledger changes it and the next request misses + recomputes. The probe is
    // a single-row read over the ledgers ordering key (CH `ORDER BY sequence
    // DESC LIMIT 1` — see `crate::common::head`), orders of magnitude cheaper
    // than the stats statement it guards (task 0291).
    let head = head::latest_sequence_ch(&state.ch()).await;
    let head = match head {
        Ok(head) => head,
        Err(e) => {
            tracing::error!(error = %e, "DB error reading head in get_network_stats");
            // Availability: the head read is a new hard dependency in front of
            // the cache (it did not exist under the old TTL design, where a
            // warm HIT served with no DB round-trip). A transient head-read
            // failure (DB blip, CH `read_rows` quota) must not 500 a request a
            // warm cache could still answer — serve the last good snapshot if
            // we have one (slightly stale but internally consistent).
            return match state.network_last_good.read().unwrap().clone() {
                Some(stats) => ok_response(stats),
                None => errors::internal_error(
                    errors::DB_ERROR,
                    "Unable to retrieve network statistics.",
                ),
            };
        }
    };

    // Conditional GET (task 0292): the head IS the ETag, so an `If-None-Match`
    // that already names it means the client's cached body is current — return
    // `304 Not Modified` (empty body) BEFORE the heavy stats statement. The
    // short-circuit sits after the cheap head read and before `try_get_with`,
    // so an idle poll costs only the head probe. (On a head-read failure above
    // we never reach here — that path serves the last-good snapshot.)
    if conditional::if_none_match_satisfied(&headers, head) {
        // Weak tag: the stats body is not byte-stable for a given head (it
        // carries `generated_at`), so a strong validator would be incorrect.
        return conditional::not_modified_weak(head);
    }

    // `try_get_with` deduplicates concurrent misses on the same head: only
    // the first task runs the DB query, every other concurrent task on the
    // same key waits for that task's result. Errors are propagated as
    // `Arc<FetchStatsError>` so a single failed fetch is not cached and the
    // next request retries cleanly. The stats statement pins its latest-ledger
    // row to `head`, so the value stored under key `head` always reports
    // `latest_ledger_sequence == head`.
    let result: Result<Arc<NetworkStats>, Arc<clickhouse::error::Error>> = state
        .network_cache
        .try_get_with(head, async {
            let stats = queries::fetch_stats(&state.ch(), head)
                .await
                .map(Arc::new)?;
            // Runs only on a miss (inside the initializer): record the freshest
            // successfully-computed snapshot for the head-read failure fallback
            // above. The std `RwLock` write is never held across an `.await`.
            *state.network_last_good.write().unwrap() = Some(Arc::clone(&stats));
            Ok(stats)
        })
        .await;

    match result {
        Ok(stats) => ok_response(stats),
        Err(e) => {
            tracing::error!(error = %e, "DB error in get_network_stats");
            errors::internal_error(errors::DB_ERROR, "Unable to retrieve network statistics.")
        }
    }
}

fn ok_response(stats: Arc<NetworkStats>) -> Response {
    // ETag is derived from the body's own head, not the separately-read head:
    // the two are equal on the normal path (the stats statement pins its row to
    // `head`, task 0291), and on the head-read-failure fallback the body is the
    // last-good snapshot, so its `latest_ledger_sequence` is the head this
    // response actually represents. This keeps the validator consistent with
    // the bytes we send (task 0292).
    let head = stats.latest_ledger_sequence;
    let mut resp = Json(stats).into_response();
    cache_control::attach(&mut resp, cache_control::LIVE);
    // Weak tag (see the 304 path): same head can yield byte-different bodies
    // (`generated_at`, cache recompute, last-good fallback), so a strong
    // validator would violate RFC 7232 §2.1.
    conditional::attach_weak_etag(&mut resp, head);
    resp
}

#[cfg(test)]
mod tests;
