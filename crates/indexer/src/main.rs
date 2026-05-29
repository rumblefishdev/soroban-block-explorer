//! Ledger Processor Lambda for the Soroban block explorer.
//!
//! Processes LedgerCloseMeta payloads from S3 and persists structured
//! data to ClickHouse on Hetzner (via the Caddy mTLS reverse proxy).
//!
//! Task 0241 — PG → CH hard swap.

mod handler;

use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use lambda_runtime::{Error, service_fn};
use tracing::info;

/// Production CH database — the writer module's `init.sql` creates
/// every table under the `default` database (`crates/db-clickhouse/schema/init.sql`).
const CH_DATABASE: &str = "default";

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "unknown".to_string());
    let mtls_secret_name = std::env::var("MTLS_SECRET_NAME").unwrap_or_default();
    let ch_domain = std::env::var("CH_DOMAIN").unwrap_or_default();
    info!(
        env_name = %env_name,
        ch_domain = %ch_domain,
        mtls_secret_name = %mtls_secret_name,
        "indexer cold start — building mTLS ClickHouse client"
    );

    // Eager-init the parser's network-id cache. Surfaces a missing
    // `STELLAR_NETWORK_PASSPHRASE` as a clean Lambda Init Errors
    // failure instead of a per-event `parse_ledger` panic.
    handler::process::init_network_id().map_err(|e| format!("network_id init failed: {e}"))?;

    // Cold-start mTLS bundle fetch. Reads MTLS_SECRET_NAME + CH_DOMAIN
    // from the env (set by `infra/src/lib/stacks/compute-stack.ts`),
    // hits the Parameters and Secrets Lambda Extension on
    // localhost:2773, parses the {cert, key, ca} bundle into a rustls
    // ClientConfig, and assembles a `clickhouse::Client` against
    // https://{CH_DOMAIN}. Any failure here (missing env, extension
    // unreachable, bundle malformed) returns from `main` so Lambda
    // surfaces it via CW `Init Errors`.
    let ch_client = match db_clickhouse::mtls::client_from_lambda_env(CH_DATABASE).await {
        Ok(c) => c,
        Err(e) => {
            // Surface a structured `error!` line BEFORE propagating —
            // `lambda_runtime` only logs the error stringly, which
            // makes the operator hunt for context. With this we get a
            // structured CW entry tagged with which secret / domain
            // was attempted, instrumentable via CW Logs Insights.
            tracing::error!(
                env_name = %env_name,
                ch_domain = %ch_domain,
                mtls_secret_name = %mtls_secret_name,
                error = %e,
                "failed to build mTLS CH client"
            );
            return Err(format!("failed to build mTLS CH client: {e}").into());
        }
    };
    info!("mTLS ClickHouse client ready");

    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3_client = S3Client::new(&aws_config);
    let cw_client = CloudWatchClient::new(&aws_config);
    let sqs_client = SqsClient::new(&aws_config);

    let enrichment_publisher = handler::enrichment_publish::Publisher::from_env(sqs_client)?;

    // The doorbell handler derives S3 keys from ledger numbers and reads them
    // from this bucket (it does not parse the S3 event). CDK always injects
    // `BUCKET_NAME` (`compute-stack.ts`); a missing value fails init loudly.
    let bucket = std::env::var("BUCKET_NAME").unwrap_or_default();
    if bucket.is_empty() {
        return Err("BUCKET_NAME env var is missing or empty".into());
    }

    let state = handler::HandlerState {
        s3_client,
        bucket,
        cw_client,
        ch_client,
        enrichment_publisher,
    };

    info!("indexer ready — starting Lambda runtime");

    lambda_runtime::run(service_fn(|event| handler::handler(event, &state))).await
}
