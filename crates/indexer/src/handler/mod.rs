//! Lambda handler for the Ledger Processor.
//!
//! Receives S3 PutObject events, downloads and decompresses XDR files,
//! orchestrates the four parsing stages, and persists each ledger via
//! [`db_clickhouse::persist::persist_ledger_clickhouse`] — the same
//! one-shot wrapper the runner's `Sink::persist_ledger` fallback drives
//! (open `PartitionWriter` → write_ledger → commit per ledger). Per-
//! ledger commit isolation: a failure on ledger N inside an S3 batch
//! does not roll back already-committed ledgers 1..N-1; the Lambda
//! returns Err on the first hard failure and S3 redelivery re-attempts
//! the batch from scratch (`ReplacingMergeTree` dedupes the 17 RMT
//! tables; `ledgers` stays plain MergeTree by design, see
//! `live-tail-cutover.md` B-2 for the operator-side duplicate
//! mitigation).
//!
//! Task 0241: the legacy 15-step PG flow (`handler::persist`) is now
//! feature-gated behind `pg-persist`. The production Lambda binary
//! drives CH only and contains no PG / sqlx code.

pub mod enrichment_publish;
#[cfg(feature = "pg-persist")]
pub mod persist;
pub mod process;

use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_s3::Client as S3Client;
use db_clickhouse::persist::persist_ledger_clickhouse;
use lambda_runtime::{Error, LambdaEvent};
use serde::Deserialize;
use std::time::Duration;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// S3 event types (subset of the full Lambda S3 event schema)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct S3Event {
    #[serde(rename = "Records")]
    pub records: Vec<S3EventRecord>,
}

#[derive(Debug, Deserialize)]
pub struct S3EventRecord {
    pub s3: S3Entity,
}

#[derive(Debug, Deserialize)]
pub struct S3Entity {
    pub bucket: S3Bucket,
    pub object: S3Object,
}

#[derive(Debug, Deserialize)]
pub struct S3Bucket {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct S3Object {
    pub key: String,
}

// ---------------------------------------------------------------------------
// Handler error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("S3 download failed: {0}")]
    S3Download(String),
    #[error("parse error: {0}")]
    Parse(#[from] xdr_parser::ParseError),
    #[error("ClickHouse write failed: {0}")]
    ClickHouse(#[from] db_clickhouse::SchemaError),
    /// Legacy PG persist path — only constructible when the `pg-persist`
    /// feature is enabled (backfill-runner / backfill-bench dev tools).
    #[cfg(feature = "pg-persist")]
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[cfg(feature = "pg-persist")]
    #[error("persist staging error: {0}")]
    Staging(String),
}

// ---------------------------------------------------------------------------
// CH retry strategy
// ---------------------------------------------------------------------------

/// Mirrors the PG retry backoff shape — three retry attempts (four
/// wire calls total) with the same 50/200/800 ms cadence so operator
/// dashboards keep the same recovery envelope when something flaps on
/// the wire.
///
/// Applied per ledger inside the S3 batch — a transient hiccup on
/// ledger 3 retries just that ledger; ledgers 1–2 are already
/// committed. Hard error after retries → Lambda returns Err → S3
/// redelivery re-attempts the whole batch (`ReplacingMergeTree`
/// dedupes the 17 RMT tables on next merge).
const RETRY_BACKOFF_MS: [u64; 3] = [50, 200, 800];

// ---------------------------------------------------------------------------
// Shared state passed to every invocation
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct HandlerState {
    pub s3_client: S3Client,
    pub cw_client: CloudWatchClient,
    /// ClickHouse client (mTLS to Hetzner via Caddy). Construction lives
    /// in `main.rs` cold start; cloning is cheap (the underlying
    /// `hyper-util` pool is `Arc`-shared).
    pub ch_client: clickhouse::Client,
    /// Type-1 enrichment SQS publisher (task 0191). Currently runs in
    /// **stub mode** — the lookup queries against PG no longer execute
    /// (PG is frozen post-cutover). The publisher is retained at the
    /// CDK + IAM layer pending the CH-aware rewrite called out in
    /// `enrichment_publish.rs`.
    pub enrichment_publisher: enrichment_publish::Publisher,
}

// ---------------------------------------------------------------------------
// Lambda entry point
// ---------------------------------------------------------------------------

/// Top-level Lambda handler. Processes each S3 record independently.
pub async fn handler(event: LambdaEvent<S3Event>, state: &HandlerState) -> Result<(), Error> {
    let (payload, _ctx) = event.into_parts();

    let total = payload.records.len();
    let mut skipped = 0usize;

    for record in &payload.records {
        let bucket = &record.s3.bucket.name;
        // S3 event keys are URL-encoded (e.g. slashes as %2F, spaces as +).
        let key = percent_encoding::percent_decode_str(&record.s3.object.key)
            .decode_utf8_lossy()
            .into_owned();

        info!(bucket, key = key.as_str(), "processing S3 record");

        // Validate S3 key pattern (must match Galexie filename format).
        let ledger_range = match xdr_parser::parse_s3_key(&key) {
            Ok(range) => range,
            Err(e) => {
                warn!(bucket, key = key.as_str(), error = %e, "skipping non-matching S3 key");
                skipped += 1;
                continue;
            }
        };

        match process_s3_object(state, bucket, &key, ledger_range).await {
            Ok(()) => {
                info!(
                    bucket,
                    key = key.as_str(),
                    start = ledger_range.0,
                    end = ledger_range.1,
                    "S3 record processed",
                );
            }
            Err(e) => {
                let safe = safe_error_message(&e);
                error!(bucket, key = key.as_str(), error = %safe, "failed to process S3 record");
                // Hand the SANITISED string to lambda_runtime, not the
                // raw `HandlerError` — `Diagnostic`'s `From<Error>`
                // serializes the full `Display` to the Lambda error
                // report (→ CloudWatch), which would re-leak the CH
                // body that `safe` already stripped.
                return Err(safe.into());
            }
        }
    }

    if skipped > 0 && skipped == total {
        error!(
            total,
            skipped, "all S3 records skipped by parse_s3_key — no data persisted"
        );
    }

    Ok(())
}

/// Download, decompress, parse, and persist one S3 object containing a
/// batch of ledgers. Each ledger goes through its own
/// [`persist_ledger_clickhouse`] call (open writer → write → commit),
/// retried on transient HTTP errors with `[50, 200, 800] ms` backoff.
/// A hard failure on ledger N returns Err — already-committed ledgers
/// 1..N-1 stay in CH; S3 redelivery re-attempts the whole batch and
/// `ReplacingMergeTree` dedupes the 17 RMT tables.
async fn process_s3_object(
    state: &HandlerState,
    bucket: &str,
    key: &str,
    _ledger_range: (u32, u32),
) -> Result<(), HandlerError> {
    // Step 1: Download from S3
    let compressed = download_s3_object(&state.s3_client, bucket, key).await?;

    // Step 2: Decompress
    let xdr_bytes = xdr_parser::decompress_zstd(&compressed)?;

    // Step 3: Deserialize batch
    let batch = xdr_parser::deserialize_batch(&xdr_bytes)?;

    // Step 4: Per-ledger parse + persist with retry envelope. Aggregate
    // the enrichment-relevant slices so we pay the SQS lookup +
    // SendMessageBatch round-trips once per S3 record rather than once
    // per ledger (matches the pre-cutover handler shape; today both
    // publishes are stubbed, see `enrichment_publish.rs`).
    let mut batch_extracted_assets: Vec<xdr_parser::types::ExtractedAsset> = Vec::new();
    let mut nft_mint_ledgers: Vec<u32> = Vec::new();

    for ledger_meta in batch.ledger_close_metas.iter() {
        let parsed = process::parse_ledger(ledger_meta);
        let ledger_sequence = parsed.ledger.sequence;
        let had_nft_mints = parsed.nfts.iter().any(|n| n.minted_at_ledger.is_some());

        persist_with_retry(&state.ch_client, &parsed).await?;
        publish_ledger_sequence_metric(&state.cw_client, ledger_sequence).await;

        if had_nft_mints {
            nft_mint_ledgers.push(ledger_sequence);
        }
        batch_extracted_assets.extend(parsed.assets);
    }

    // Enrichment publish — stub today (no PG, no CH lookup yet). See
    // `enrichment_publish.rs` for re-enablement plan. Failures here
    // never abort the batch — the CH writes are already durable.
    state
        .enrichment_publisher
        .publish_for_extracted_assets(&batch_extracted_assets)
        .await;
    state
        .enrichment_publisher
        .publish_for_minted_nfts(&nft_mint_ledgers)
        .await;

    Ok(())
}

/// Wrap one `persist_ledger_clickhouse` call in the retry envelope.
/// Transient HTTP errors (network drop, timeout, Caddy / CH 5xx,
/// CH server-side transient exception codes) retry with backoff;
/// non-transient errors fail loud. Thin wrapper over
/// [`retry_with_backoff`] so the retry policy itself stays testable
/// without a live `clickhouse::Client`.
async fn persist_with_retry(
    client: &clickhouse::Client,
    parsed: &process::ParseOutput,
) -> Result<(), HandlerError> {
    let ledger_sequence = parsed.ledger.sequence;
    let tx_count = parsed.transactions.len();

    let attempts = retry_with_backoff(
        ledger_sequence,
        &RETRY_BACKOFF_MS,
        is_transient_ch_error,
        || async {
            persist_ledger_clickhouse(
                client,
                &parsed.ledger,
                &parsed.transactions,
                &parsed.operations,
                &parsed.events,
                &parsed.invocations,
                &parsed.contract_interfaces,
                &parsed.contract_deployments,
                &parsed.account_states,
                &parsed.liquidity_pools,
                &parsed.pool_snapshots,
                &parsed.assets,
                &parsed.nfts,
                &parsed.nft_events,
                &parsed.lp_positions,
                &parsed.contract_name_writes,
                &parsed.sac_overrides,
            )
            .await
            .map_err(HandlerError::ClickHouse)
        },
    )
    .await?;

    // Single success log per ledger. The recovery vs steady-state
    // distinction lives here (not inside `retry_with_backoff`) so we
    // emit exactly one line and keep `tx_count` in it — emitting both
    // a generic "persisted" and a "persisted after retry" would
    // double-count any throughput metric keyed on the message.
    if attempts > 0 {
        info!(
            ledger_sequence,
            tx_count,
            attempts = attempts + 1,
            "ledger persisted after retry"
        );
    } else {
        info!(ledger_sequence, tx_count, "ledger persisted");
    }
    Ok(())
}

/// Render a `HandlerError` for logging / error propagation with **all
/// server-supplied detail stripped**.
///
/// Logging policy: logs carry diagnostic metadata only — error class,
/// ClickHouse exception code, HTTP status — and **never** any
/// server-supplied body. `clickhouse::error::Error::BadResponse`
/// echoes the verbatim server response, which for a rejected INSERT
/// includes the offending row's values (account StrKeys, balances,
/// asset codes). Even though that data is public on-chain, our policy
/// is zero private/business data in logs: a log line exists to point
/// the operator at the failure class, not to dump record contents.
///
/// We therefore NEVER call `Display`/`to_string()` on an inner error
/// (which could embed a body); we emit fixed labels plus, for
/// `BadResponse`, only the leading `Code: NNN` / HTTP-status token
/// extracted by [`safe_bad_response_token`]. This string is used both
/// in our `error!`/`warn!` macros and as the value `return Err(…)`
/// hands to `lambda_runtime` (which would otherwise log the full
/// `Display` via its `Diagnostic` serialization, bypassing any
/// in-macro redaction).
fn safe_error_message(err: &HandlerError) -> String {
    match err {
        HandlerError::S3Download(_) => "S3 download failed".to_string(),
        HandlerError::Parse(_) => "XDR parse failed".to_string(),
        HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(ch)) => match ch {
            clickhouse::error::Error::Network(_) => "ClickHouse network error".to_string(),
            clickhouse::error::Error::TimedOut => "ClickHouse request timed out".to_string(),
            clickhouse::error::Error::BadResponse(msg) => {
                format!("ClickHouse bad response ({})", safe_bad_response_token(msg))
            }
            // Any other clickhouse error variant (Custom, serde, etc.)
            // could carry a payload fragment in its Display — emit a
            // bare label, never the contents.
            _ => "ClickHouse error".to_string(),
        },
        HandlerError::ClickHouse(db_clickhouse::SchemaError::Staging(_)) => {
            "ClickHouse staging error".to_string()
        }
        #[cfg(feature = "pg-persist")]
        HandlerError::Database(_) => "database error".to_string(),
        #[cfg(feature = "pg-persist")]
        HandlerError::Staging(_) => "persist staging error".to_string(),
    }
}

/// Extract ONLY the leading code/status token from a CH `BadResponse`
/// body — never the trailing detail, which can carry row values.
/// Returns `"Code: NNN"` for a CH exception body, `"HTTP NNN"` for a
/// plain HTTP status line, or `"detail suppressed"` for anything else
/// (a proxy HTML page, an opaque body) where we cannot prove the
/// remainder is data-free.
fn safe_bad_response_token(msg: &str) -> String {
    // CH exception body: "Code: NNN. DB::Exception: …".
    if let Some(rest) = msg.strip_prefix("Code: ") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            return format!("Code: {digits}");
        }
    }
    // Plain HTTP status line: "NNN <reason>". Only the 3-digit status
    // is guaranteed data-free; the reason phrase / any proxy body is
    // not, so keep just the numeric status.
    let leading: String = msg.chars().take_while(char::is_ascii_digit).collect();
    if leading.len() == 3 {
        return format!("HTTP {leading}");
    }
    "detail suppressed".to_string()
}

/// Generic retry envelope — drives `op` until it returns `Ok` or the
/// backoff schedule is exhausted. `is_transient` decides which errors
/// are eligible to retry; non-transient errors return immediately.
/// On success returns the number of retries performed (0 = succeeded
/// on the first attempt) so the caller can log a single recovery-aware
/// success line.
///
/// Pulled out of `persist_with_retry` so the loop control (attempt
/// counter, backoff sleep, transient classification) can be unit-
/// tested against a fake `op` that yields a scripted sequence of
/// `Result`s — the production caller uses the real
/// `persist_ledger_clickhouse` future.
///
/// **Note on `Send`**: no `Send` bound on `Fut` is intentional. The
/// Lambda binary drives this future from the single
/// `lambda_runtime::run` task; we never `tokio::spawn` it onto a
/// different worker. The `ParseOutput` reference captured by the
/// production closure is also `!Send` in some compositions, so
/// requiring `Send` here would force needless `Arc` wrappers
/// upstream. If a future caller needs to spawn, add the bound at
/// that callsite, not here.
async fn retry_with_backoff<F, Fut>(
    ledger_sequence: u32,
    backoff_ms: &[u64],
    is_transient: fn(&HandlerError) -> bool,
    mut op: F,
) -> Result<usize, HandlerError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), HandlerError>>,
{
    let mut attempt = 0usize;
    loop {
        match op().await {
            Ok(()) => return Ok(attempt),
            Err(err) => {
                if attempt < backoff_ms.len() && is_transient(&err) {
                    let delay = Duration::from_millis(backoff_ms[attempt]);
                    warn!(
                        ledger_sequence,
                        attempt,
                        backoff_ms = delay.as_millis() as u64,
                        error = %safe_error_message(&err),
                        "ledger persist hit transient CH error — retrying"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                return Err(err);
            }
        }
    }
}

/// Best-effort CW metric publish. Failures are warn-logged and do not
/// abort the batch — the metric is operator visibility only; the
/// authoritative state of `LastProcessedLedgerSequence` is the
/// `ledgers` table in CH.
async fn publish_ledger_sequence_metric(cw_client: &CloudWatchClient, ledger_sequence: u32) {
    use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};

    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "unknown".to_string());
    let datum = MetricDatum::builder()
        .metric_name("LastProcessedLedgerSequence")
        .dimensions(
            Dimension::builder()
                .name("Environment")
                .value(&env_name)
                .build(),
        )
        .value(f64::from(ledger_sequence))
        .unit(StandardUnit::None)
        .build();
    let result = cw_client
        .put_metric_data()
        .namespace("SorobanBlockExplorer/Indexer")
        .metric_data(datum)
        .send()
        .await;
    if let Err(e) = result {
        warn!(ledger_sequence, error = %e, "failed to publish LastProcessedLedgerSequence metric");
    }
}

/// Classify a `HandlerError` as transient (eligible for the retry
/// envelope) vs hard (fail-loud). Mirrors the PG `retry_delay` shape
/// (40001 / 40P01 → transient) for the CH world.
///
/// * **Transport** — `Error::Network(_)` (hyper / IO failure on the
///   wire), `Error::TimedOut` (request-level timeout) → always retry.
/// * **`BadResponse`** — see [`is_retryable_bad_response`]: a denylist
///   of definitively permanent failures (HTTP 4xx, CH semantic
///   exception codes); everything else is retried.
/// * Other variants (serialization, config) → fail loud.
fn is_transient_ch_error(err: &HandlerError) -> bool {
    let HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(ch_err)) = err else {
        return false;
    };
    match ch_err {
        clickhouse::error::Error::Network(_) | clickhouse::error::Error::TimedOut => true,
        clickhouse::error::Error::BadResponse(msg) => is_retryable_bad_response(msg),
        _ => false,
    }
}

/// Decide whether a `BadResponse` message is worth retrying.
///
/// **Denylist, not allowlist.** The `clickhouse` 0.15 crate does NOT
/// surface the HTTP status code separately: `BadResponse` carries the
/// server / proxy response **body verbatim** when it is non-empty, and
/// only falls back to `"NNN <Canonical-Reason>"` for empty bodies (see
/// `clickhouse-0.15.0/src/response.rs::collect_bad_response` +
/// `reason`). So a prefix match like `starts_with("502 ")` only catches
/// the *empty-body* case — a Caddy / CH 5xx that carries any body (or a
/// CH server error rendered as `"Code: NNN. DB::Exception…"`) would slip
/// through an allowlist and skip the retry envelope, tripping the DLQ on
/// routine infra turbulence.
///
/// For the live-tail indexer the INSERT payload is deterministic and
/// well-formed, so a `BadResponse` is almost always infra turbulence
/// (Caddy reload → 5xx, CH overload / restart, proxy hiccup). We
/// therefore **retry by default** and only fail loud on errors a retry
/// genuinely cannot fix: HTTP 4xx client errors and CH semantic
/// exception codes (schema / type / parse / auth). Worst case for a
/// permanent-but-unrecognised error is three wasted retries (~1 s) then
/// the same DLQ terminal state — availability over a marginal latency
/// cost.
fn is_retryable_bad_response(msg: &str) -> bool {
    !is_permanent_bad_response(msg)
}

/// HTTP 4xx prefixes the `clickhouse` crate renders for empty-body
/// client errors (`reason()` → `"NNN <Canonical-Reason>"`). Retrying a
/// 4xx never helps — bad request shape, auth, or routing.
const HTTP_4XX_PREFIXES: &[&str] = &[
    "400 ", "401 ", "403 ", "404 ", "405 ", "409 ", "413 ", "414 ", "415 ", "422 ", "431 ",
];

/// CH server-side exception codes that are deterministic for a given
/// INSERT — schema / query / auth problems a retry cannot fix. NOT
/// exhaustive: it's the set a well-formed indexer INSERT could
/// realistically hit (a mis-migrated schema, a cert mapped to the wrong
/// CH user, a parser/serialization bug). Unlisted codes fall through to
/// "retry" — acceptable, since a genuinely permanent unlisted code just
/// burns the retry budget before the DLQ.
const CH_PERMANENT_CODES: &[u32] = &[
    16,  // NO_SUCH_COLUMN_IN_TABLE
    27,  // CANNOT_PARSE_INPUT_ASSERTION_FAILED
    36,  // BAD_ARGUMENTS — malformed argument the same INSERT always reproduces
    47,  // UNKNOWN_IDENTIFIER
    53,  // TYPE_MISMATCH
    60,  // UNKNOWN_TABLE
    62,  // SYNTAX_ERROR
    73,  // UNKNOWN_FORMAT
    117, // INCORRECT_DATA — row fails CH-side validation (bad Decimal, enum, NULL into non-Null col); retry never fixes a deterministic row shape
    192, // UNKNOWN_USER
    193, // WRONG_PASSWORD
    194, // REQUIRED_PASSWORD
    497, // ACCESS_DENIED
    516, // AUTHENTICATION_FAILED
         // NOT listed: 49 LOGICAL_ERROR — ClickHouse raises it as a
         // catch-all for internal invariant violations, some of which are
         // transient (mid-mutation races, dictionary warmup after a
         // restart). Classifying it permanent would DLQ a ledger that a
         // retry could have landed; per the availability-over-latency
         // policy a code with any realistic transient case stays in the
         // default-retry path (worst case: a few wasted retries before the
         // DLQ on a genuinely permanent occurrence).
];

/// True when the message is a definitively permanent failure (HTTP 4xx
/// or a known CH semantic exception code).
fn is_permanent_bad_response(msg: &str) -> bool {
    if HTTP_4XX_PREFIXES.iter().any(|p| msg.starts_with(p)) {
        return true;
    }
    CH_PERMANENT_CODES
        .iter()
        .any(|code| ch_code_matches(msg, *code))
}

/// Match a CH exception body's leading `"Code: NNN"` token with a
/// trailing non-digit boundary, so `Code: 6` does not match `Code: 60`
/// and `Code: 159` does not match `Code: 1590`. CH bodies render as
/// `"Code: NNN. DB::Exception: …"`; the test fixtures also exercise the
/// `"Code: NNN, …"` comma form.
fn ch_code_matches(msg: &str, code: u32) -> bool {
    let needle = format!("Code: {code}");
    match msg.strip_prefix(needle.as_str()) {
        Some(rest) => !rest.starts_with(|c: char| c.is_ascii_digit()),
        None => false,
    }
}

/// Download an object from S3.
async fn download_s3_object(
    client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, HandlerError> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| HandlerError::S3Download(e.to_string()))?;

    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| HandlerError::S3Download(e.to_string()))?
        .into_bytes()
        .to_vec();

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // -------------------------------------------------------------------
    // retry_with_backoff — generic loop control
    // -------------------------------------------------------------------

    fn always_transient(_err: &HandlerError) -> bool {
        true
    }

    fn never_transient(_err: &HandlerError) -> bool {
        false
    }

    /// Cook up a fast `HandlerError` variant that doesn't construct CH
    /// internals — the classifier in `is_transient_ch_error` does its
    /// own job under separate tests; here we want pure loop-control
    /// scripting.
    fn dummy_err() -> HandlerError {
        HandlerError::S3Download("test-transient".into())
    }

    #[tokio::test]
    async fn retry_loop_success_on_first_try() {
        let calls = RefCell::new(0usize);
        let res = retry_with_backoff(
            42,
            &[1, 1, 1], // tiny backoffs to keep tests fast
            always_transient,
            || async {
                *calls.borrow_mut() += 1;
                Ok::<(), HandlerError>(())
            },
        )
        .await;
        // Returns 0 retries performed (succeeded on first attempt).
        assert_eq!(res.unwrap(), 0);
        assert_eq!(*calls.borrow(), 1);
    }

    #[tokio::test]
    async fn retry_loop_recovers_after_transient_errors() {
        let calls = RefCell::new(0usize);
        let res = retry_with_backoff(7, &[1, 1, 1], always_transient, || async {
            let n = {
                let mut c = calls.borrow_mut();
                *c += 1;
                *c
            };
            if n < 3 { Err(dummy_err()) } else { Ok(()) }
        })
        .await;
        // Succeeded on the 3rd call → 2 retries performed.
        assert_eq!(res.unwrap(), 2);
        assert_eq!(*calls.borrow(), 3);
    }

    #[tokio::test]
    async fn retry_loop_exhausts_after_max_attempts() {
        let calls = RefCell::new(0usize);
        let res = retry_with_backoff(99, &[1, 1, 1], always_transient, || async {
            *calls.borrow_mut() += 1;
            Err::<(), _>(dummy_err())
        })
        .await;
        assert!(res.is_err());
        // 1 initial + 3 retries = 4 wire calls total.
        assert_eq!(*calls.borrow(), 4);
    }

    #[tokio::test]
    async fn retry_loop_fails_loud_on_non_transient() {
        let calls = RefCell::new(0usize);
        let res = retry_with_backoff(11, &[1, 1, 1], never_transient, || async {
            *calls.borrow_mut() += 1;
            Err::<(), _>(dummy_err())
        })
        .await;
        assert!(res.is_err());
        // First call only — no retries because classifier rejects.
        assert_eq!(*calls.borrow(), 1);
    }

    #[tokio::test]
    async fn retry_loop_empty_backoff_means_zero_retries() {
        let calls = RefCell::new(0usize);
        let res = retry_with_backoff(0, &[], always_transient, || async {
            *calls.borrow_mut() += 1;
            Err::<(), _>(dummy_err())
        })
        .await;
        assert!(res.is_err());
        assert_eq!(*calls.borrow(), 1);
    }

    // -------------------------------------------------------------------
    // is_retryable_bad_response — classifier substrings
    // -------------------------------------------------------------------

    /// 5xx surfaces — both the empty-body `"NNN <reason>"` shape and
    /// (crucially) the non-empty-body shapes that an allowlist prefix
    /// match would have missed.
    #[test]
    fn is_retryable_bad_response_recognises_5xx() {
        // Empty-body form rendered by the crate's `reason()`.
        assert!(is_retryable_bad_response("502 Bad Gateway"));
        assert!(is_retryable_bad_response("503 Service Unavailable"));
        assert!(is_retryable_bad_response("504 Gateway Timeout"));
        assert!(is_retryable_bad_response("507 Insufficient Storage"));
        // Non-empty-body forms — the regression the denylist fixes.
        // Caddy / proxy 5xx carrying an HTML or text body, and a CH
        // server-side 5xx rendered as a verbatim body. An allowlist
        // prefix("502 ") would have returned false here.
        assert!(is_retryable_bad_response(
            "<html><head><title>502 Bad Gateway</title></head>...</html>"
        ));
        assert!(is_retryable_bad_response(
            "upstream connect error or disconnect/reset before headers"
        ));
    }

    #[test]
    fn is_retryable_bad_response_recognises_ch_transient_codes() {
        assert!(is_retryable_bad_response(
            "Code: 159, DB::Exception: TIMEOUT_EXCEEDED ..."
        ));
        assert!(is_retryable_bad_response(
            "Code: 252, DB::Exception: TOO_MANY_PARTS ..."
        ));
        assert!(is_retryable_bad_response("Code: 999"));
        // CH bodies use a trailing period after the code.
        assert!(is_retryable_bad_response(
            "Code: 202. DB::Exception: Too many simultaneous queries"
        ));
    }

    #[test]
    fn is_retryable_bad_response_skips_semantic_errors() {
        // 4xx — client error, retry won't help.
        assert!(!is_retryable_bad_response("400 Bad Request"));
        assert!(!is_retryable_bad_response("403 Forbidden"));
        assert!(!is_retryable_bad_response("404 Not Found"));
        // Type / parse / table / auth errors from CH — semantic, no retry.
        assert!(!is_retryable_bad_response(
            "Code: 60, DB::Exception: UNKNOWN_TABLE ..."
        ));
        assert!(!is_retryable_bad_response(
            "Code: 27, DB::Exception: CANNOT_PARSE_INPUT_ASSERTION_FAILED ..."
        ));
        assert!(!is_retryable_bad_response(
            "Code: 53, DB::Exception: TYPE_MISMATCH ..."
        ));
        assert!(!is_retryable_bad_response(
            "Code: 516. DB::Exception: ... AUTHENTICATION_FAILED"
        ));
    }

    /// Data-shape errors are deterministic for a given INSERT — a retry
    /// re-sends the same bad row and fails identically. They must be
    /// classed permanent so the budget isn't burned before the DLQ.
    #[test]
    fn is_retryable_bad_response_skips_data_shape_errors() {
        assert!(!is_retryable_bad_response(
            "Code: 117. DB::Exception: INCORRECT_DATA: Cannot parse ..."
        ));
        assert!(!is_retryable_bad_response(
            "Code: 36, DB::Exception: BAD_ARGUMENTS ..."
        ));
    }

    /// `49 LOGICAL_ERROR` is a CH catch-all with transient cases
    /// (mid-mutation race, dictionary warmup) — it must NOT be classed
    /// permanent, so a transient occurrence still gets the retry
    /// envelope rather than going straight to the DLQ.
    #[test]
    fn is_retryable_bad_response_retries_logical_error() {
        assert!(is_retryable_bad_response(
            "Code: 49. DB::Exception: LOGICAL_ERROR ..."
        ));
    }

    /// `safe_error_message` must emit ONLY the CH code token for a
    /// rejected-INSERT error — never the body detail that carries row
    /// values. This is the zero-private-data-in-logs guarantee.
    #[test]
    fn safe_error_message_strips_ch_body_detail() {
        // Body shaped like a real INCORRECT_DATA rejection echoing a
        // (synthetic) account value in the detail.
        let body = "Code: 117. DB::Exception: INCORRECT_DATA: \
                    Cannot parse 'GSYNTHETICACCOUNTVALUE' as Decimal in column balance";
        let err = HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(
            clickhouse::error::Error::BadResponse(body.to_string()),
        ));
        let rendered = safe_error_message(&err);
        // Code retained for triage…
        assert_eq!(rendered, "ClickHouse bad response (Code: 117)");
        // …and the row value / detail is GONE.
        assert!(!rendered.contains("GSYNTHETICACCOUNTVALUE"));
        assert!(!rendered.contains("balance"));
        assert!(!rendered.contains("Cannot parse"));
    }

    #[test]
    fn safe_bad_response_token_extracts_code_or_status_only() {
        assert_eq!(
            safe_bad_response_token("Code: 159. DB::Exception: TIMEOUT_EXCEEDED: detail"),
            "Code: 159"
        );
        assert_eq!(safe_bad_response_token("502 Bad Gateway"), "HTTP 502");
        // Opaque proxy body with no recognisable leading token → fully
        // suppressed (no passthrough of arbitrary content).
        assert_eq!(
            safe_bad_response_token("<html>upstream sent value GABC...</html>"),
            "detail suppressed"
        );
    }

    #[test]
    fn safe_error_message_uses_fixed_labels_no_detail() {
        // Inner strings must never appear in the rendered label.
        let s3 = HandlerError::S3Download("bucket=secret key=ledgers/123".into());
        assert_eq!(safe_error_message(&s3), "S3 download failed");
        assert!(!safe_error_message(&s3).contains("secret"));

        let net = HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(
            clickhouse::error::Error::TimedOut,
        ));
        assert_eq!(safe_error_message(&net), "ClickHouse request timed out");
    }

    /// Prefix-boundary: `Code: 6` must not match `Code: 60`, and a
    /// transient `Code: 159` must not be shadowed by a permanent
    /// `Code: 1` / `Code: 16` entry (and vice-versa).
    #[test]
    fn ch_code_boundary_is_exact() {
        // `60` is permanent; `600` (hypothetical, unlisted) is not —
        // boundary check must not let `60` swallow `600`.
        assert!(is_retryable_bad_response(
            "Code: 600. DB::Exception: hypothetical transient"
        ));
        // `16` permanent must not match `169` (unlisted → retry).
        assert!(is_retryable_bad_response("Code: 169. DB::Exception: x"));
        // Exact permanent still caught.
        assert!(!is_retryable_bad_response("Code: 16. DB::Exception: x"));
    }

    /// Unrecognised / opaque BadResponse bodies default to retryable
    /// — availability over a marginal retry cost (the live-tail INSERT
    /// is deterministic, so an opaque error is almost always infra).
    #[test]
    fn is_retryable_bad_response_defaults_unknown_to_retry() {
        assert!(is_retryable_bad_response("some opaque caddy body"));
        assert!(is_retryable_bad_response(""));
        assert!(is_retryable_bad_response("Code: 241 MEMORY_LIMIT_EXCEEDED"));
    }
}
