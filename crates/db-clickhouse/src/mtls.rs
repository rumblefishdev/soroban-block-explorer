//! mTLS-authenticated ClickHouse client (task 0241 Part A §5).
//!
//! Builds a [`clickhouse::Client`] backed by a `hyper-util` HTTP client
//! whose connector presents a client certificate to the Caddy reverse
//! proxy in front of the Hetzner CH (per task 0240's per-service RBAC
//! model — Caddy validates the cert chain, maps the cert CN to a CH
//! user via `CLICKHOUSE_CN_USER_MAP`, and injects
//! `X-ClickHouse-User: <mapped-user>` on the upstream request).
//!
//! The bundle (`cert`, `key`, `ca`) is fetched at Lambda cold start
//! from AWS Secrets Manager through the
//! [AWS Parameters and Secrets Lambda Extension](https://docs.aws.amazon.com/secretsmanager/latest/userguide/retrieving-secrets_lambda.html)
//! (`http://localhost:2773`). The extension is layered onto every
//! mTLS-bearing Lambda by `infra/src/lib/stacks/compute-stack.ts`. We
//! prefer the extension over the SDK so repeat reads in a warm Lambda
//! container hit the in-process cache (env
//! `PARAMETERS_SECRETS_EXTENSION_CACHE_ENABLED=true`, also set by CDK),
//! not Secrets Manager — no SM API call on the hot path.
//!
//! `X-ClickHouse-User` and Basic Auth are intentionally NOT set on the
//! client side. Caddy's mTLS terminator strips both
//! (`request_header -X-ClickHouse-User`, `uri query -user` /
//! `uri query -password`) and re-applies the CN-mapped identity. Any
//! client-side credential would be discarded; setting one would only
//! invite drift if Caddy's behaviour ever changed under our feet.

use std::io::Cursor;
use std::time::Duration;

use rustls::ClientConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// PEM-encoded mTLS material as it lands in the Secrets Manager bundle.
///
/// The CN of `cert` selects the CH user via Caddy's `CLICKHOUSE_CN_USER_MAP`
/// (per `infra-hetzner/Caddyfile`). Issue + upload procedure lives in
/// `infra-hetzner/ca/README.md` and is exercised by the operator steps
/// in `docs/runbooks/live-tail-cutover.md` (D-4).
#[derive(Clone)]
pub struct MtlsBundle {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
}

impl std::fmt::Debug for MtlsBundle {
    /// Manual impl to avoid leaking the private key (or any PEM material)
    /// into traces / panic backtraces. The bundle is sensitive enough
    /// that even cert / CA fingerprints are better fetched on demand
    /// than blanket-logged.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MtlsBundle")
            .field("cert_pem", &"<redacted>")
            .field("key_pem", &"<redacted>")
            .field("ca_pem", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MtlsError {
    #[error("required env var `{0}` is missing or empty")]
    MissingEnv(&'static str),
    #[error("Secrets Extension fetch failed: {0}")]
    Fetch(String),
    #[error("Secrets bundle JSON decode failed: {0}")]
    BundleDecode(String),
    #[error("PEM parse failed: {0}")]
    PemParse(String),
    #[error("rustls config error: {0}")]
    Rustls(String),
}

#[derive(serde::Deserialize)]
struct BundleJson {
    cert: String,
    key: String,
    ca: String,
}

/// HTTP path served by the Parameters and Secrets Lambda Extension.
/// Pinned to the version that ships with the layer ARN listed in
/// `infra/src/lib/mtls.ts` (`Arm64:78` for eu-central-1). The endpoint
/// has been stable across all extension versions to date; if AWS ever
/// rev's the path, this is the one constant to update.
const EXTENSION_URL: &str = "http://localhost:2773/secretsmanager/get";

/// Header the extension requires for authn — the in-process token is
/// the same `AWS_SESSION_TOKEN` Lambda already injects into the runtime.
const EXTENSION_TOKEN_HEADER: &str = "X-Aws-Parameters-Secrets-Token";

/// Connect timeout for the localhost extension HTTP request. The
/// extension binds a Unix-local TCP socket; a working layer answers
/// SYN within single-digit ms. Half a second leaves headroom for cold
/// container readiness without making Lambda init hang on a
/// misconfigured layer ARN (the default `reqwest::Client::new()` has
/// no connect timeout — a missing layer would block until the Lambda
/// function timeout, surfacing as `Task timed out` instead of the
/// more actionable `MtlsError::Fetch`).
const EXTENSION_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

/// Overall request timeout. The bundle JSON is small (≤ 16 KiB even
/// for typical PEM material) and the extension's in-process cache
/// returns warm responses in tens of milliseconds; 2 s is a generous
/// upper bound for both cold and warm fetches and still well below
/// any Lambda function timeout we configure.
const EXTENSION_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Fetch the mTLS bundle from the Parameters and Secrets Lambda
/// Extension. Cold-start cost is one HTTP round-trip to localhost; warm
/// invocations in the same execution environment hit the extension's
/// in-process cache.
///
/// Reads `AWS_SESSION_TOKEN` from the environment for the
/// `X-Aws-Parameters-Secrets-Token` header. Lambda always sets that
/// variable; a missing value points at a non-Lambda runtime (e.g. local
/// dev) and is surfaced as [`MtlsError::MissingEnv`] so the operator
/// can route to a different bundle loader.
pub async fn fetch_bundle_from_extension(secret_name: &str) -> Result<MtlsBundle, MtlsError> {
    let token = require_env("AWS_SESSION_TOKEN")?;
    let client = reqwest::Client::builder()
        .connect_timeout(EXTENSION_CONNECT_TIMEOUT)
        .timeout(EXTENSION_REQUEST_TIMEOUT)
        .build()
        .map_err(|e| MtlsError::Fetch(format!("reqwest client build failed: {e}")))?;
    let resp = client
        .get(EXTENSION_URL)
        .query(&[("secretId", secret_name)])
        .header(EXTENSION_TOKEN_HEADER, token)
        .send()
        .await
        .map_err(|e| {
            MtlsError::Fetch(format!(
                "extension at {EXTENSION_URL} unreachable (verify Parameters and Secrets \
                 layer ARN is attached and layer is initialised): {e}"
            ))
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(MtlsError::Fetch(format!(
            "extension returned HTTP {status} (secret={secret_name})"
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| MtlsError::Fetch(format!("response body parse failed: {e}")))?;
    let secret_string = body
        .get("SecretString")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MtlsError::Fetch("response missing `SecretString` field".into()))?;
    let parsed: BundleJson =
        serde_json::from_str(secret_string).map_err(|e| MtlsError::BundleDecode(e.to_string()))?;
    Ok(MtlsBundle {
        cert_pem: parsed.cert,
        key_pem: parsed.key,
        ca_pem: parsed.ca,
    })
}

/// Assemble a [`clickhouse::Client`] that talks HTTPS + mTLS to
/// `https://{domain}`. The client is cheap to clone — under the hood
/// `hyper_util::client::legacy::Client` owns an `Arc`-shared connection
/// pool, so cloning the returned `clickhouse::Client` shares the pool.
///
/// `database` is the CH database to send queries against. Pass `"default"`
/// for the production layout (see `crates/db-clickhouse/schema/init.sql`).
pub fn client_with_mtls(
    domain: &str,
    bundle: &MtlsBundle,
    database: &str,
) -> Result<clickhouse::Client, MtlsError> {
    install_default_crypto_provider();

    let cert_chain = parse_certs(&bundle.cert_pem)?;
    if cert_chain.is_empty() {
        return Err(MtlsError::PemParse(
            "client cert PEM contained zero certs".into(),
        ));
    }
    let key = parse_private_key(&bundle.key_pem)?;

    // Server-cert trust store — used by the client to verify the *server's*
    // certificate during the TLS handshake. Caddy fronts ClickHouse with a
    // Let's Encrypt cert (chains to a public root), so we MUST seed the
    // Mozilla WebPKI roots; the original implementation seeded only
    // `bundle.ca_pem`, so every handshake failed server-cert verification of
    // the LE chain → `Error::Network` ("ClickHouse network error") on every
    // persist. We additionally keep the bundle CA so an internal-PKI server
    // cert would also verify (defence-in-depth; harmless alongside WebPKI).
    // Note: `bundle.ca_pem` is primarily the CA that signed *our client*
    // cert, which the server (Caddy `client_auth`) verifies — not us.
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for ca in parse_certs(&bundle.ca_pem)? {
        roots
            .add(ca)
            .map_err(|e| MtlsError::Rustls(format!("RootCertStore::add failed: {e}")))?;
    }

    let tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(cert_chain, key)
        .map_err(|e| MtlsError::Rustls(format!("with_client_auth_cert failed: {e}")))?;

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_only()
        .enable_http1()
        .build();

    let hyper_client =
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            // ClickHouse server-side default keep-alive is 10 s on CH ≥ 23.11
            // (we run CH 26.x). 8 s leaves a margin so we close idle sockets
            // before the server does — keeps the connection pool from holding
            // half-dead sockets that surface as `connection closed` on the next
            // request after a quiet period.
            .pool_idle_timeout(Duration::from_secs(8))
            // The indexer writes ledger-by-ledger but the Lambda is sized for
            // reservedConcurrentExecutions ≤ a handful. Two pooled connections
            // per host is plenty for serial writes; bumping this hides
            // connection-error symptoms but uses more SM Extension warm-cert
            // memory across reuse.
            .pool_max_idle_per_host(2)
            .build(https);

    let url = format!("https://{domain}");
    Ok(clickhouse::Client::with_http_client(hyper_client)
        .with_url(url)
        .with_database(database))
}

/// One-shot convenience for Lambda cold start: read the bundle from the
/// Parameters and Secrets Lambda Extension (using `MTLS_SECRET_NAME`)
/// and assemble the CH client against `CH_DOMAIN`.
///
/// Errors with [`MtlsError::MissingEnv`] if either variable is unset —
/// CDK always sets both (`infra/src/lib/stacks/compute-stack.ts`), so
/// a missing value fails Lambda init and surfaces as a CW `Init Errors`
/// metric, not a half-configured client that silently fails per-request.
pub async fn client_from_lambda_env(database: &str) -> Result<clickhouse::Client, MtlsError> {
    let secret_name = require_env("MTLS_SECRET_NAME")?;
    let domain = require_env("CH_DOMAIN")?;
    let bundle = fetch_bundle_from_extension(&secret_name).await?;
    client_with_mtls(&domain, &bundle, database)
}

fn require_env(key: &'static str) -> Result<String, MtlsError> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(MtlsError::MissingEnv(key)),
    }
}

fn parse_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>, MtlsError> {
    let mut reader = Cursor::new(pem.as_bytes());
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| MtlsError::PemParse(format!("certs(): {e}")))
}

fn parse_private_key(pem: &str) -> Result<PrivateKeyDer<'static>, MtlsError> {
    let mut reader = Cursor::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| MtlsError::PemParse(format!("private_key(): {e}")))?
        .ok_or_else(|| MtlsError::PemParse("no private key found in PEM".into()))
}

/// rustls 0.23 requires an explicit crypto provider install before
/// `ClientConfig::builder()`. `aws-lc-rs` is enabled via cargo feature
/// (see `aws-mtls` in `Cargo.toml`); install the default provider once
/// per process. Idempotent — repeat calls are a no-op (`set_default`'s
/// `Err` branch is benign: "another thread installed the default
/// first").
fn install_default_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three concatenated PEM-encoded certs from the same on-disk
    /// well-formed test bundle would be ideal here; we only verify the
    /// parser shape (zero / non-zero result) since the actual TLS path
    /// is exercised end-to-end by the cutover smoke test, not in unit
    /// tests.
    #[test]
    fn parse_certs_rejects_empty_pem() {
        assert!(parse_certs("").map(|v| v.is_empty()).unwrap_or(true));
    }

    #[test]
    fn require_env_errors_when_unset() {
        let r = require_env("DEFINITELY_NOT_SET_VAR_FOR_TEST_0241");
        assert!(matches!(r, Err(MtlsError::MissingEnv(_))));
    }

    #[test]
    fn bundle_debug_redacts_pem_material() {
        let b = MtlsBundle {
            cert_pem: "SECRET-CERT".into(),
            key_pem: "SECRET-KEY".into(),
            ca_pem: "SECRET-CA".into(),
        };
        let rendered = format!("{b:?}");
        assert!(!rendered.contains("SECRET-CERT"));
        assert!(!rendered.contains("SECRET-KEY"));
        assert!(!rendered.contains("SECRET-CA"));
    }
}
