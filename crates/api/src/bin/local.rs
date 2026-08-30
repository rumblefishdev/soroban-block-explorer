//! Local development server — the API surface over plain axum, talking to the
//! PRODUCTION ClickHouse through the Caddy mTLS endpoint with a developer
//! client certificate (the same `client_with_mtls` path the Lambda uses; the
//! cert CN maps to the `dev_shared` CH user via Caddy's CN→user map).
//!
//! READ-ONLY by construction: the API only issues SELECTs. This binary exists
//! so a feature can be eyeballed against real data before deploy — it is NOT
//! a deployment target (no edge lock, no auth gate, no CORS; pair it with the
//! Vite dev proxy so the browser stays same-origin).
//!
//! ```text
//! # 1. API against prod CH (this binary)
//! cargo run -p api --bin local
//!
//! # 2. point the SPA's dev proxy at it — web/.env.development.local
//! #    (gitignored; DEV_API_KEY is only needed for the prod-API target)
//! VITE_API_BASE_URL=http://localhost:4200
//! DEV_API_PROXY_TARGET=http://localhost:9100
//!
//! # 3. serve the SPA
//! npx nx run @rumblefish/soroban-block-explorer-web:dev
//! ```
//!
//! The browser then only ever talks to `localhost:4200` (same-origin, no
//! CORS) and Vite forwards `/v1` here. Both local pieces are machine-scoped
//! and deliberately untracked — nothing to commit to run this.
//!
//! Env (all optional):
//! - `LOCAL_CH_URL` — plain (non-mTLS) ClickHouse HTTP endpoint, e.g. the
//!   docker-compose CH at `http://localhost:8125`. When set, the mTLS vars
//!   below are ignored and credentials come from the standard
//!   `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` / `CLICKHOUSE_DATABASE` —
//!   the e2e-testing path (task 0374): real API over a locally ingested CH.
//! - `LOCAL_MTLS_DIR` — dir with `<user>.crt`, `<user>.key`, `ca.crt`
//!   (default `infra-hetzner/ca/out/$USER`)
//! - `LOCAL_CH_DOMAIN` — Caddy mTLS host (default `ch.sorobanscan.rumblefish.dev`)
//! - `LOCAL_PORT` — listen port (default 9100)

use api::runtime_enrichment::RuntimeEnrichment;
use api::runtime_enrichment::sep1::Sep1Fetcher;
use api::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use api::{AppState, openapi};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let user = std::env::var("USER").unwrap_or_else(|_| "dev".into());
    let mtls_dir =
        std::env::var("LOCAL_MTLS_DIR").unwrap_or_else(|_| format!("infra-hetzner/ca/out/{user}"));
    let domain =
        std::env::var("LOCAL_CH_DOMAIN").unwrap_or_else(|_| "ch.sorobanscan.rumblefish.dev".into());
    let port: u16 = std::env::var("LOCAL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9100);

    // Plain-CH branch first (task 0374 e2e): a locally ingested docker CH
    // needs no certs, and reaching for the mTLS bundle there fails on
    // missing PEMs before the URL is even considered.
    let ch = if let Ok(url) = std::env::var("LOCAL_CH_URL") {
        let cfg = db_clickhouse::Config {
            url,
            ..db_clickhouse::Config::from_env()
        };
        tracing::info!("plain ClickHouse client → {}", cfg.url);
        db_clickhouse::client(&cfg)
    } else {
        let read = |name: String| {
            std::fs::read_to_string(&name)
                .unwrap_or_else(|e| panic!("failed to read mTLS PEM {name}: {e}"))
        };
        let bundle = db_clickhouse::mtls::MtlsBundle {
            cert_pem: read(format!("{mtls_dir}/{user}.crt")),
            key_pem: read(format!("{mtls_dir}/{user}.key")),
            ca_pem: read(format!("{mtls_dir}/ca.crt")),
        };
        db_clickhouse::mtls::client_with_mtls(&domain, &bundle, db_clickhouse::PROD_DATABASE)
            .expect("failed to build mTLS ClickHouse client")
    };

    let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .no_credentials()
        .region(aws_sdk_s3::config::Region::new("us-east-2"))
        .load()
        .await;
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(aws_sdk_s3::Client::new(&aws_config)),
        sep1: Sep1Fetcher::new().expect("failed to build SEP-1 fetcher"),
        nft_token_uri: api::runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("failed to build NFT token_uri fetcher"),
        wasm_code: api::runtime_enrichment::wasm_code::WasmCodeFetcher::new()
            .expect("failed to build wasm-code RPC client"),
    };

    let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE")
        .unwrap_or_else(|_| "Public Global Stellar Network ; September 2015".into());
    let network_id = xdr_parser::network_id(&passphrase);

    let state = AppState::new(ch, runtime_enrichment, network_id);
    // Routes only — no edge lock / auth / CORS. Same registration the Lambda
    // router and the OpenAPI codegen share, so paths cannot drift.
    let (router, _spec) = openapi::register_routes()
        .with_state(state)
        .split_for_parts();

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("local api listening on http://{addr} → CH {domain} (user via cert CN)");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind local port");
    axum::serve(listener, router)
        .await
        .expect("local server failed");
}
