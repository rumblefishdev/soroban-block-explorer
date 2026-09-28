//! Read-time fetch of raw `.xdr.zst` ledger files from the public Stellar archive.
//!
//! Implements the read-path component of ADR 0029: heavy fields (memo, signatures,
//! full event topics/data, XDR blobs) not persisted in the DB are pulled on-demand
//! from `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/` at request time.
//!
//! Callers pass a slice of ledger sequences and receive
//! `Vec<Result<LedgerCloseMeta, FetchError>>` in input order, so each requested
//! ledger may succeed or fail independently. Downloads run concurrently up to
//! `MAX_CONCURRENT_FETCHES`. No caching — follow-up task if needed.

pub mod dto;
pub mod extractors;
pub mod key;
pub mod merge;

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::timeout::TimeoutConfig;
use futures::stream::{self, StreamExt};
use std::time::Duration;
use stellar_xdr::LedgerCloseMeta;
use thiserror::Error;
use tracing::instrument;

use self::key::build_s3_key;

/// Public Stellar data archive bucket. No credentials required.
pub const PUBLIC_ARCHIVE_BUCKET: &str = "aws-public-blockchain";

/// S3 key prefix inside the bucket for pubnet ledgers.
pub const PUBLIC_ARCHIVE_PREFIX: &str = "v1.1/stellar/ledgers/pubnet";

/// Default per-request budget for public-archive S3 GETs. Chosen so that an
/// end-to-end E3/E14 request completes well under API Gateway's 29s limit
/// even with a retry or fallback upstream.
pub const DEFAULT_S3_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of concurrent in-flight GETs issued by `fetch_ledgers`.
/// Caps CPU/connection spikes when a caller passes a large slice (e.g.,
/// a full E14 page whose events reference many distinct ledgers).
pub const MAX_CONCURRENT_FETCHES: usize = 16;

/// Build a timeout config applied to every public-archive S3 request.
pub fn default_timeout_config() -> TimeoutConfig {
    TimeoutConfig::builder()
        .operation_timeout(DEFAULT_S3_OPERATION_TIMEOUT)
        .operation_attempt_timeout(DEFAULT_S3_OPERATION_TIMEOUT)
        .build()
}

/// Errors returned by the fetcher.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("ledger {seq} not found in public archive")]
    NotFound { seq: u32 },

    #[error("S3 error fetching ledger {seq}: {source}")]
    S3 {
        seq: u32,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("decompression failed for ledger {seq}: {source}")]
    Decompress {
        seq: u32,
        #[source]
        source: xdr_parser::ParseError,
    },

    #[error("XDR deserialization failed for ledger {seq}: {source}")]
    Deserialize {
        seq: u32,
        #[source]
        source: xdr_parser::ParseError,
    },

    #[error("empty batch returned for ledger {seq}")]
    EmptyBatch { seq: u32 },

    #[error("ledger {seq} parse task failed to join: {source}")]
    Join {
        seq: u32,
        #[source]
        source: tokio::task::JoinError,
    },
}

/// Fetches raw `.xdr.zst` ledger files from the public Stellar archive.
///
/// Stateless and cheap to clone (`S3Client` is `Arc`-backed internally).
/// Create once per Lambda via shared state; reuse across requests.
#[derive(Clone)]
pub struct StellarArchiveFetcher {
    client: S3Client,
}

impl StellarArchiveFetcher {
    /// Construct a fetcher from a pre-configured unsigned S3 client.
    pub fn new(client: S3Client) -> Self {
        Self { client }
    }

    /// Fetch, decompress, and deserialize a single ledger.
    ///
    /// Returns the first `LedgerCloseMeta` in the batch — the public archive
    /// writes one ledger per file, so there is never more than one.
    #[instrument(skip(self), fields(ledger_seq = seq))]
    pub async fn fetch_ledger(&self, seq: u32) -> Result<LedgerCloseMeta, FetchError> {
        let key = format!("{PUBLIC_ARCHIVE_PREFIX}/{}", build_s3_key(seq));

        let compressed = self.download(seq, &key).await?;

        // zstd decompress (~1.5 MB) + full-batch XDR deserialize is synchronous
        // CPU work; offload it to the blocking pool so it does not monopolise
        // the async worker thread during the parse (which would block the
        // reactor — including any co-`join!`ed future on the caller side, see
        // `transactions::handlers::get_transaction`).
        tokio::task::spawn_blocking(move || {
            let xdr_bytes = xdr_parser::decompress_zstd(compressed.as_ref())
                .map_err(|source| FetchError::Decompress { seq, source })?;
            let batch = xdr_parser::deserialize_batch(&xdr_bytes)
                .map_err(|source| FetchError::Deserialize { seq, source })?;

            let metas: Vec<LedgerCloseMeta> = batch.ledger_close_metas.into();
            metas
                .into_iter()
                .next()
                .ok_or(FetchError::EmptyBatch { seq })
        })
        .await
        // A `JoinError` here means the parse closure panicked (the task is never
        // cancelled): surface it honestly as `Join`, not as an S3/IO error.
        // Upstream this degrades to `heavy_fields_status = unavailable` like any
        // other fetch failure rather than unwinding the request.
        .map_err(|source| FetchError::Join { seq, source })?
    }

    /// Fetch multiple ledgers concurrently. Results are returned in input order.
    ///
    /// Concurrency is capped at `MAX_CONCURRENT_FETCHES` to prevent connection
    /// and CPU spikes when callers pass a large slice. On any per-ledger
    /// failure the corresponding slot contains `Err` — callers decide how to
    /// handle partial failure (e.g. fail fast, degrade gracefully).
    #[instrument(skip(self, seqs), fields(count = seqs.len()))]
    pub async fn fetch_ledgers(&self, seqs: &[u32]) -> Vec<Result<LedgerCloseMeta, FetchError>> {
        stream::iter(seqs.iter().copied())
            .map(|seq| self.fetch_ledger(seq))
            .buffered(MAX_CONCURRENT_FETCHES)
            .collect()
            .await
    }

    async fn download(&self, seq: u32, key: &str) -> Result<bytes::Bytes, FetchError> {
        let resp = self
            .client
            .get_object()
            .bucket(PUBLIC_ARCHIVE_BUCKET)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                // Only promote to `NotFound` for a genuine service-level 404.
                // Other failure modes (timeouts, dispatch, credential
                // resolution) keep the original SDK error as the source so
                // callers/log consumers retain context.
                if matches!(e.as_service_error(), Some(svc) if svc.is_no_such_key()) {
                    FetchError::NotFound { seq }
                } else {
                    FetchError::S3 {
                        seq,
                        source: Box::new(e),
                    }
                }
            })?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| FetchError::S3 {
                seq,
                source: Box::new(e),
            })?
            .into_bytes();

        Ok(bytes)
    }
}

#[cfg(test)]
pub(crate) fn test_client() -> S3Client {
    use aws_smithy_runtime_api::client::http::{
        HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings, SharedHttpConnector,
    };
    use aws_smithy_runtime_api::client::orchestrator::HttpRequest;
    use aws_smithy_runtime_api::client::result::ConnectorError;
    use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;

    #[derive(Debug)]
    struct NoTrafficHttpClient;

    impl HttpClient for NoTrafficHttpClient {
        fn http_connector(
            &self,
            _settings: &HttpConnectorSettings,
            _components: &RuntimeComponents,
        ) -> SharedHttpConnector {
            SharedHttpConnector::new(NoTrafficConnector)
        }
    }

    #[derive(Debug)]
    struct NoTrafficConnector;

    impl HttpConnector for NoTrafficConnector {
        fn call(&self, _request: HttpRequest) -> HttpConnectorFuture {
            HttpConnectorFuture::new(async {
                Err(ConnectorError::user(
                    std::io::Error::other("test S3 client does not allow network traffic").into(),
                ))
            })
        }
    }

    let aws_cfg = aws_sdk_s3::config::Builder::new()
        .region(aws_sdk_s3::config::Region::new("us-east-2"))
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            "test-access-key",
            "test-secret-key",
            None,
            None,
            "stellar_archive_test_client",
        ))
        .timeout_config(default_timeout_config())
        .http_client(NoTrafficHttpClient)
        .build();

    S3Client::from_conf(aws_cfg)
}

#[cfg(test)]
#[path = "tests/stellar_archive_tests.rs"]
mod tests;
