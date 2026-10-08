//! Ledger Processor Lambda for the Soroban block explorer.
//!
//! Processes LedgerCloseMeta payloads from S3 and persists structured
//! data to ClickHouse on Hetzner (via the Caddy mTLS reverse proxy).
//!
//! Task 0241 — PG → CH hard swap.

mod handler;
// The handler reaches it as `crate::token_metadata_by_functions` in both the
// library and this binary; here it is the library's copy, not a second one.
use indexer::token_metadata_by_functions;

use aws_sdk_cloudwatch::Client as CloudWatchClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_sqs::Client as SqsClient;
use lambda_runtime::{Error, service_fn};
use tracing::info;

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
    let ch_client = match db_clickhouse::mtls::client_from_lambda_env(
        &db_clickhouse::database_from_env(),
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            // Surface a structured `error!` line BEFORE propagating —
            // `lambda_runtime` only logs the error stringly, which
            // makes the operator hunt for context. With this we get a
            // structured CW entry tagged with which secret / domain
            // was attempted, instrumentable via CW Logs Insights.
            // `alarm` is a machine contract shared with the
            // `indexer-ch-write-failures` metric filter — see the
            // matching comment in `handler/mod.rs::handler`.
            tracing::error!(
                alarm = "ch_write_failure",
                cause = "mtls_init",
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
    let cw_client = CloudWatchClient::new(&aws_config);
    let sqs_client = SqsClient::new(&aws_config);

    let enrichment_publisher =
        handler::enrichment_publish::Publisher::from_env(sqs_client.clone(), ch_client.clone())?;

    // The doorbell handler derives S3 keys from ledger numbers and reads them
    // from this bucket (it does not parse the S3 event). CDK always injects
    // `BUCKET_NAME` (`compute-stack.ts`); a missing value fails init loudly.
    let bucket = std::env::var("BUCKET_NAME").unwrap_or_default();
    if bucket.is_empty() {
        return Err("BUCKET_NAME env var is missing or empty".into());
    }

    // Mainnet reads its own Galexie bucket at the root. Testnet names the
    // public data lake instead (lore-0553): unsigned, in its own region, under
    // this network's folder — which must match the passphrase, or every
    // transaction would hash wrong without an error.
    xdr_parser::public_archive::check_configured_archive()?;
    check_folder_needs_lake(
        &bucket,
        xdr_parser::public_archive::configured_archive_prefix().as_deref(),
    )?;
    // The lake sends no events: reading it, the indexer also queues its own
    // next wake-up (`Pacer`).
    let (s3_client, key_prefix, pacer) = if bucket == xdr_parser::public_archive::PUBLIC_BUCKET {
        let public = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .no_credentials()
            .region(aws_sdk_s3::config::Region::new(
                xdr_parser::public_archive::PUBLIC_BUCKET_REGION,
            ))
            .load()
            .await;
        let prefix = format!("{}/", xdr_parser::public_archive::public_archive_prefix());
        let pacer = handler::lake_pacing::Pacer::from_env(sqs_client)?;
        (S3Client::new(&public), prefix, Some(pacer))
    } else {
        (S3Client::new(&aws_config), String::new(), None)
    };

    let state = handler::HandlerState {
        s3_client,
        bucket,
        key_prefix,
        cw_client,
        ch_client,
        enrichment_publisher,
        // Task 0283 live G9 — fresh per cold start; warms across invocations.
        classification_cache: domain::ClassificationCache::new(),
        pacer,
    };

    info!("indexer ready — starting Lambda runtime");

    lambda_runtime::run(service_fn(|event| handler::handler(event, &state))).await
}

/// A ledger folder is read only inside the public data lake; our own Galexie
/// bucket keeps ledgers at its root. A folder configured next to our own
/// bucket would be ignored without a word, so that pairing is refused.
fn check_folder_needs_lake(bucket: &str, configured_prefix: Option<&str>) -> Result<(), String> {
    match configured_prefix {
        Some(prefix) if bucket != xdr_parser::public_archive::PUBLIC_BUCKET => Err(format!(
            "PUBLIC_ARCHIVE_PREFIX is `{prefix}`, but BUCKET_NAME is `{bucket}`: a ledger \
             folder is read only from `{}`, so it would be ignored",
            xdr_parser::public_archive::PUBLIC_BUCKET
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "tests/main_tests.rs"]
mod tests;
