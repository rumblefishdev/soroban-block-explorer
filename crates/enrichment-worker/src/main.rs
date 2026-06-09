//! Type-1 enrichment worker Lambda.
//!
//! SQS event source mapping → batches of `SqsEvent` records → per-record
//! dispatch by `kind` to the corresponding `enrich_*` function from the
//! shared `enrichment-shared` crate.
//!
//! Per task 0191 (write path ported PG → CH, task 0231):
//! - Writes are INSERTs into the ClickHouse enrichment side tables
//!   (`asset_enrichment` / `nft_enrichment`, ADR 0048) with `version =
//!   now_ms` — `ReplacingMergeTree` keeps the latest write per key
//!   (latest-wins). The indexer-owned tables are never touched. Per-key
//!   fetch + sentinel rules live in `enrichment_shared::enrich_and_persist::*`.
//! - Batch failure model: each record is processed independently. A
//!   per-record failure is reported via `BatchItemFailures` so SQS
//!   redelivers only the failed messages, not the whole batch (the
//!   `ReportBatchItemFailures` response feature on the event source
//!   mapping).
//! - Cold start: build the mTLS ClickHouse client (same bundle path as the
//!   indexer Lambda) + a single `Sep1Fetcher` / `NftTokenUriFetcher`; reuse
//!   all three across handler invocations.
//!
//! Future kinds (e.g. `lp_tvl`) plug in by adding a variant to
//! `EnrichmentMessage` + a `match` arm + the fn in `enrichment-shared`.

use std::sync::Arc;

use aws_lambda_events::event::sqs::{BatchItemFailure, SqsBatchResponse, SqsEvent, SqsMessage};
use enrichment_shared::enrich_and_persist::nft_token_uri::enrich_nft_token_uri;
use enrichment_shared::enrich_and_persist::sep1_assets::enrich_asset_from_sep1;
use enrichment_shared::enrich_and_persist::{EnrichError, EnrichmentMessage};
use enrichment_shared::nft_token_uri::NftTokenUriFetcher;
use enrichment_shared::sep1::Sep1Fetcher;
use lambda_runtime::{Error, LambdaEvent, service_fn};
use tracing::{error, info, instrument};

struct WorkerState {
    client: clickhouse::Client,
    sep1: Sep1Fetcher,
    nft_token_uri: NftTokenUriFetcher,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    info!("enrichment-worker cold start — building mTLS ClickHouse client");

    // Same mTLS bundle path as the indexer Lambda (MTLS_SECRET_NAME +
    // CH_DOMAIN → Secrets extension → rustls client). Writes land in the
    // enrichment side tables (ADR 0048); the indexer-owned tables are
    // never touched by this worker.
    let client = db_clickhouse::mtls::client_from_lambda_env(db_clickhouse::PROD_DATABASE)
        .await
        .map_err(|e| format!("failed to build mTLS ClickHouse client: {e}"))?;
    let sep1 = Sep1Fetcher::new()?;
    let nft_token_uri = NftTokenUriFetcher::new()?;
    let state = Arc::new(WorkerState {
        client,
        sep1,
        nft_token_uri,
    });

    info!("enrichment-worker ready — starting Lambda runtime");

    lambda_runtime::run(service_fn(move |event: LambdaEvent<SqsEvent>| {
        let state = Arc::clone(&state);
        async move { handle_event(event, state).await }
    }))
    .await
}

#[instrument(skip(event, state), fields(records = event.payload.records.len()))]
async fn handle_event(
    event: LambdaEvent<SqsEvent>,
    state: Arc<WorkerState>,
) -> Result<SqsBatchResponse, Error> {
    let mut failures = Vec::new();

    for record in event.payload.records {
        let message_id = require_message_id(&record)?;
        let outcome = handle_record(&record, &state).await;
        if let Some(failure) = classify_outcome(&message_id, outcome) {
            failures.push(failure);
        }
    }

    Ok(SqsBatchResponse {
        batch_item_failures: failures,
    })
}

/// Pull a record's `message_id` or fail the whole invocation.
///
/// SQS partial-batch reporting requires `item_identifier` to match
/// the record's messageId exactly — a wrong / synthetic value is
/// treated as "successfully processed" by the broker and the record
/// is silently deleted. A missing `message_id` is a Lambda event-shape
/// contract violation (AWS always sets it); failing the whole
/// invocation forces SQS to redrive the entire batch instead of
/// risking lost enrichment attempts.
fn require_message_id(record: &SqsMessage) -> Result<String, Error> {
    match record.message_id.clone() {
        Some(id) => Ok(id),
        None => {
            error!("SQS record missing message_id; failing invocation to force batch redrive");
            Err("SQS record missing message_id".into())
        }
    }
}

/// Map a per-record outcome onto the SQS partial-batch contract.
///
/// `Ok` and `Permanent` ack the message (no `BatchItemFailure`):
/// `Permanent` errors won't recover on retry, so ack-and-log avoids
/// burning the SQS retry budget. `Transient` errors emit a
/// `BatchItemFailure` so SQS redelivers per `maxReceiveCount` and
/// only escalates to the DLQ on sustained outage.
fn classify_outcome(
    message_id: &str,
    outcome: Result<(), RecordError>,
) -> Option<BatchItemFailure> {
    match outcome {
        Ok(()) => None,
        Err(RecordError::Permanent(e)) => {
            error!(
                message_id = %message_id,
                "permanent record error: {e}; acking without retry"
            );
            None
        }
        Err(RecordError::Transient(e)) => {
            error!(
                message_id = %message_id,
                "transient enrichment failure: {e}; reporting partial batch failure"
            );
            Some(BatchItemFailure {
                item_identifier: message_id.to_owned(),
            })
        }
    }
}

async fn handle_record(record: &SqsMessage, state: &WorkerState) -> Result<(), RecordError> {
    let msg = parse_message(record)?;
    match msg {
        EnrichmentMessage::Sep1Assets(key) => {
            enrich_asset_from_sep1(&state.client, key, &state.sep1).await?;
            Ok(())
        }
        EnrichmentMessage::NftTokenUri(key) => {
            enrich_nft_token_uri(&state.client, key, &state.nft_token_uri).await?;
            Ok(())
        }
    }
}

/// Decode an `EnrichmentMessage` from an SQS record body.
///
/// Missing body, unknown `kind`, missing variant fields, and any
/// other deserialisation failure surface as `RecordError::Permanent`
/// — the producer is the only writer to this queue, so a malformed
/// body is a producer bug and retrying it won't help.
fn parse_message(record: &SqsMessage) -> Result<EnrichmentMessage, RecordError> {
    let body = record
        .body
        .as_deref()
        .ok_or_else(|| RecordError::Permanent("SQS record had no body".to_owned()))?;
    serde_json::from_str(body)
        .map_err(|e| RecordError::Permanent(format!("malformed enrichment JSON: {e}")))
}

/// Two-bucket error split mirrors the worker's retry semantics:
/// `Permanent` is acked (no retry), `Transient` triggers a SQS retry.
#[derive(Debug, thiserror::Error)]
enum RecordError {
    #[error("permanent: {0}")]
    Permanent(String),
    #[error("transient: {0}")]
    Transient(#[from] EnrichError),
}

#[cfg(test)]
mod tests {
    //! Unit tests cover the testable kernel of the worker:
    //!   - `EnrichmentMessage` deserialisation (tagged enum contract)
    //!   - `parse_message` body / JSON / kind error mapping
    //!   - `classify_outcome` ack-vs-retry decision
    //!   - `require_message_id` rejection of malformed records
    //!
    //! `handle_record` (DB + HTTP) and `handle_event` (full Lambda glue)
    //! are not covered here — they require a live `clickhouse::Client` and `Sep1Fetcher`,
    //! which are the responsibility of the per-kind tests in
    //! `enrichment-shared` and a deploy-time smoke test.
    use super::*;

    fn record(message_id: Option<&str>, body: Option<&str>) -> SqsMessage {
        SqsMessage {
            message_id: message_id.map(str::to_owned),
            receipt_handle: None,
            body: body.map(str::to_owned),
            md5_of_body: None,
            md5_of_message_attributes: None,
            attributes: Default::default(),
            message_attributes: Default::default(),
            event_source_arn: None,
            event_source: None,
            aws_region: None,
        }
    }

    // -- EnrichmentMessage serde -------------------------------------

    #[test]
    fn enrichment_message_parses_sep1_assets_variant() {
        let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":42,"contract_id":7}"#;
        let msg: EnrichmentMessage = serde_json::from_str(json).expect("parse");
        let EnrichmentMessage::Sep1Assets(key) = msg else {
            panic!("expected Sep1Assets variant, got {msg:?}");
        };
        assert_eq!(key.asset_type, 1);
        assert_eq!(key.asset_code, "USDC");
        assert_eq!(key.issuer_id, 42);
        assert_eq!(key.contract_id, 7);
    }

    #[test]
    fn enrichment_message_parses_nft_token_uri_variant() {
        let json = r#"{"kind":"nft_token_uri","contract_id":99,"token_id":"3"}"#;
        let msg: EnrichmentMessage = serde_json::from_str(json).expect("parse");
        let EnrichmentMessage::NftTokenUri(key) = msg else {
            panic!("expected NftTokenUri variant, got {msg:?}");
        };
        assert_eq!(key.contract_id, 99);
        assert_eq!(key.token_id, "3");
    }

    #[test]
    fn enrichment_message_rejects_unknown_kind() {
        // Future-kind safety — adding `lp_tvl` later requires a code
        // change here, not a silent ack-and-drop on the worker side.
        let json = r#"{"kind":"lp_tvl","pool_id":1}"#;
        assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
    }

    #[test]
    fn enrichment_message_rejects_missing_kind() {
        let json = r#"{"asset_type":1,"asset_code":"USDC","issuer_id":42,"contract_id":7}"#;
        assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
    }

    #[test]
    fn enrichment_message_rejects_missing_key_field() {
        // contract_id absent — a producer that drops a key field is a bug.
        let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":42}"#;
        assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
    }

    #[test]
    fn enrichment_message_rejects_wrong_key_type() {
        // issuer_id is i64 — a string is a producer bug.
        let json = r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":"42","contract_id":7}"#;
        assert!(serde_json::from_str::<EnrichmentMessage>(json).is_err());
    }

    // -- parse_message -----------------------------------------------

    #[test]
    fn parse_message_returns_permanent_on_missing_body() {
        let r = record(Some("m-1"), None);
        match parse_message(&r) {
            Err(RecordError::Permanent(msg)) => assert!(msg.contains("no body")),
            other => panic!("expected Permanent, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_returns_permanent_on_malformed_json() {
        let r = record(Some("m-1"), Some("{not json"));
        match parse_message(&r) {
            Err(RecordError::Permanent(msg)) => {
                assert!(msg.contains("malformed enrichment JSON"))
            }
            other => panic!("expected Permanent, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_returns_sep1_assets_on_well_formed_body() {
        let r = record(
            Some("m-1"),
            Some(
                r#"{"kind":"sep1_assets","asset_type":1,"asset_code":"USDC","issuer_id":7,"contract_id":0}"#,
            ),
        );
        let msg = parse_message(&r).expect("ok");
        let EnrichmentMessage::Sep1Assets(key) = msg else {
            panic!("expected Sep1Assets variant, got {msg:?}");
        };
        assert_eq!(key.issuer_id, 7);
    }

    // -- classify_outcome --------------------------------------------

    #[test]
    fn classify_outcome_acks_ok() {
        assert!(classify_outcome("m-1", Ok(())).is_none());
    }

    #[test]
    fn classify_outcome_acks_permanent_error() {
        let outcome = Err(RecordError::Permanent("bad json".to_owned()));
        assert!(classify_outcome("m-1", outcome).is_none());
    }

    #[test]
    fn classify_outcome_emits_partial_failure_on_transient_error() {
        let outcome = Err(RecordError::Transient(EnrichError::Transient(
            "5xx from issuer".to_owned(),
        )));
        let failure = classify_outcome("m-42", outcome).expect("partial failure");
        assert_eq!(failure.item_identifier, "m-42");
    }

    #[test]
    fn classify_outcome_emits_partial_failure_on_database_error() {
        // `Custom` is the cheapest ClickHouse error variant to construct;
        // the bucket assertion is what we care about, not the exact error.
        let outcome = Err(RecordError::Transient(EnrichError::Database(
            clickhouse::error::Error::Custom("pool timed out".to_owned()),
        )));
        let failure = classify_outcome("m-99", outcome).expect("partial failure");
        assert_eq!(failure.item_identifier, "m-99");
    }

    // -- require_message_id ------------------------------------------

    #[test]
    fn require_message_id_returns_id_when_present() {
        let r = record(Some("abc-123"), Some(""));
        assert_eq!(require_message_id(&r).expect("ok"), "abc-123");
    }

    #[test]
    fn require_message_id_errors_when_missing() {
        let r = record(None, Some(""));
        assert!(require_message_id(&r).is_err());
    }
}
