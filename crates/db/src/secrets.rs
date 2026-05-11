//! AWS Secrets Manager credential resolution for RDS.
//!
//! Shared by all Lambda binaries (db-migrate, api, indexer) that connect
//! to PostgreSQL through RDS Proxy with Secrets Manager credentials.

use aws_sdk_secretsmanager::Client as SecretsClient;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

/// Resolve a `DATABASE_URL` from Secrets Manager and an RDS endpoint.
///
/// The secret (identified by `secret_arn`) must contain `username` and
/// `password` fields in its JSON body (standard RDS secret format).
/// The database name is always `soroban_explorer`.
///
/// Returns a `postgres://` connection string with `sslmode=require`
/// (required by RDS Proxy).
pub async fn resolve_database_url(
    secret_arn: &str,
    rds_endpoint: &str,
) -> Result<String, ResolveError> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = SecretsClient::new(&config);

    let response = client
        .get_secret_value()
        .secret_id(secret_arn)
        .send()
        .await
        .map_err(|e| ResolveError::SecretsManager(e.to_string()))?;

    let secret_string = response
        .secret_string()
        .ok_or(ResolveError::MissingField("secret_string"))?;

    let secret: serde_json::Value =
        serde_json::from_str(secret_string).map_err(ResolveError::Json)?;

    let username = secret["username"]
        .as_str()
        .ok_or(ResolveError::MissingField("username"))?;
    let password = secret["password"]
        .as_str()
        .ok_or(ResolveError::MissingField("password"))?;

    let encoded_username = utf8_percent_encode(username, NON_ALPHANUMERIC);
    let encoded_password = utf8_percent_encode(password, NON_ALPHANUMERIC);

    Ok(format!(
        "postgres://{encoded_username}:{encoded_password}@{rds_endpoint}:5432/soroban_explorer?sslmode=require"
    ))
}

/// Resolve a `DATABASE_URL` from the environment, falling back to
/// Secrets Manager + RDS Proxy endpoint.
///
/// Resolution order (matches the existing api / indexer / worker
/// behaviour, hoisted into one helper so all three Lambda binaries
/// share a single source of truth):
///
/// 1. `DATABASE_URL` env var — used directly when present.
/// 2. `SECRET_ARN` + `RDS_PROXY_ENDPOINT` env vars — fed into
///    [`resolve_database_url`].
///
/// Returns [`ResolveError::MissingEnvVar`] when neither path is
/// satisfiable. Lambda binaries should map this to their preferred
/// init-error shape (`Error` for `lambda_runtime`, `expect` for the
/// axum binary).
pub async fn resolve_or_env() -> Result<String, ResolveError> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }
    let secret_arn =
        std::env::var("SECRET_ARN").map_err(|_| ResolveError::MissingEnvVar("SECRET_ARN"))?;
    let rds_endpoint = std::env::var("RDS_PROXY_ENDPOINT")
        .map_err(|_| ResolveError::MissingEnvVar("RDS_PROXY_ENDPOINT"))?;
    resolve_database_url(&secret_arn, &rds_endpoint).await
}

#[derive(Debug)]
pub enum ResolveError {
    SecretsManager(String),
    Json(serde_json::Error),
    MissingField(&'static str),
    /// Neither `DATABASE_URL` nor the (`SECRET_ARN` + `RDS_PROXY_ENDPOINT`)
    /// fallback was satisfiable. The contained `&'static str` names the
    /// first env var that was missing in the fallback chain.
    MissingEnvVar(&'static str),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecretsManager(e) => write!(f, "Secrets Manager error: {e}"),
            Self::Json(e) => write!(f, "failed to parse secret JSON: {e}"),
            Self::MissingField(field) => write!(f, "missing field in secret: {field}"),
            Self::MissingEnvVar("SECRET_ARN") => {
                write!(f, "either DATABASE_URL or SECRET_ARN must be set",)
            }
            Self::MissingEnvVar(name) => {
                write!(f, "{name} must be set when DATABASE_URL is not provided",)
            }
        }
    }
}

impl std::error::Error for ResolveError {}
