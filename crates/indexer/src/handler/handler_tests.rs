use super::*;
use std::cell::RefCell;

// -------------------------------------------------------------------
// ledger_s3_key — Galexie datastore key derivation (correctness-critical:
// a wrong key reads as a gap and stalls the tail)
// -------------------------------------------------------------------

#[test]
fn ledger_s3_key_matches_galexie_scheme() {
    // Verified against a live key observed in the cutover S3 bucket.
    assert_eq!(
        ledger_s3_key(62_528_059),
        "FC45E5FF--62528000-62591999/FC45E5C4--62528059.xdr.zst"
    );
}

#[test]
fn ledger_s3_key_partition_boundaries() {
    // First ledger of a partition: partition start == ledger.
    assert_eq!(
        ledger_s3_key(62_528_000),
        "FC45E5FF--62528000-62591999/FC45E5FF--62528000.xdr.zst"
    );
    // Last ledger of the same 64000-wide partition (62528000 + 63999).
    assert_eq!(
        ledger_s3_key(62_591_999),
        "FC45E5FF--62528000-62591999/FC44EC00--62591999.xdr.zst"
    );
    // First ledger of the next partition rolls the partition prefix.
    assert!(ledger_s3_key(62_592_000).starts_with("FC44EBFF--62592000-62655999/"));
}

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

/// The full error Display must survive into the rendered string —
/// the sanitizer that used to strip it made the 2026-07-29 outage
/// undiagnosable. Guards against a well-meaning re-introduction.
#[test]
fn handler_error_display_keeps_ch_detail() {
    let body = "Code: 252. DB::Exception: Too many parts (5000)";
    let err = HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(
        clickhouse::error::Error::BadResponse(body.to_string()),
    ));
    let rendered = err.to_string();
    assert!(rendered.contains("Code: 252"));
    assert!(rendered.contains("Too many parts"));
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
