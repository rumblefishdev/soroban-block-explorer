//! Handler checks for `/verification` and `/verified-source` (task 0639).
//!
//! The paths that end before the database run everywhere. The full lookup
//! runs against a throwaway database (the real `init.sql`) and a stub Query
//! API, gated on `CH_URL` like the other DB-backed tests:
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api verified_source

use axum::body;
use axum::http::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path as url_path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::runtime_enrichment::{self, RuntimeEnrichment};

/// A real testnet contract id — the handlers validate the StrKey.
const CONTRACT: &str = "CD37GB5NIHBI5EGUNIJXEHDNVDDG23HJAVSJTAKSMDQQVKJMBSK6E5EW";
const DB: &str = "api_test_0639_verified_source";

fn state(
    ch: clickhouse::Client,
    source_verification: Option<SourceVerificationFetcher>,
) -> AppState {
    let unused = || vec!["http://unused".to_owned()];
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: runtime_enrichment::stellar_archive::StellarArchiveFetcher::new(
            runtime_enrichment::stellar_archive::test_client(),
        ),
        sep1: runtime_enrichment::sep1::Sep1Fetcher::new().expect("build sep1 fetcher"),
        nft_token_uri: runtime_enrichment::nft_token_uri::NftTokenUriFetcher::with_rpc_url(
            "http://unused".to_owned(),
        )
        .expect("build nft_token_uri fetcher"),
        wasm_code: runtime_enrichment::wasm_code::WasmCodeFetcher::with_rpc_urls(unused())
            .expect("build wasm_code fetcher"),
        account_sponsors:
            runtime_enrichment::account_sponsors::AccountSponsorsFetcher::with_rpc_urls(unused())
                .expect("build account_sponsors fetcher"),
        source_verification,
    };
    AppState::for_tests(ch, runtime_enrichment)
}

async fn json_of(resp: Response) -> (StatusCode, Option<String>, Value) {
    let status = resp.status();
    let cache = resp
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, cache, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn no_verifier_service_is_unavailable_before_any_query() {
    // Unconnected client: reaching the database would fail the test.
    let st = state(clickhouse::Client::default(), None);

    let resp = get_verification(State(st.clone()), Path(CONTRACT.to_owned())).await;
    let (status, cache, body) = json_of(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=300"));
    assert_eq!(body["status"], "unavailable");
    assert_eq!(body["contract_id"], CONTRACT);
    assert!(body["wasm_hash"].is_null(), "{body}");

    let resp = get_verified_source(State(st), Path(CONTRACT.to_owned())).await;
    let (status, _, body) = json_of(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("not available on this network"),
        "{body}"
    );
}

#[test]
fn the_response_must_fit_a_lambda_response() {
    assert!(fits_lambda_response(&vec![b'x'; MAX_RESPONSE_BYTES]));
    assert!(!fits_lambda_response(&vec![b'x'; MAX_RESPONSE_BYTES + 1]));
    assert!(fits_lambda_response(b""));
    // Each `"` and `\` is escaped again inside the proxy response.
    assert_eq!(proxy_body_len(br#"{"a":"\n"}"#), 15);
    assert!(!fits_lambda_response(&vec![
        b'"';
        MAX_RESPONSE_BYTES / 2 + 1
    ]));
}

fn files_body(content: String) -> VerifiedSourceResponse {
    VerifiedSourceResponse {
        contract_id: CONTRACT.into(),
        wasm_hash: "aa".repeat(32),
        archive_sha256: "bb".repeat(32),
        files: vec![VerifiedSourceFile {
            path: "source/src/lib.rs".into(),
            size: content.len() as u64,
            content: Some(content),
        }],
    }
}

#[tokio::test]
async fn files_are_served_as_json_with_the_long_tier() {
    let (status, cache, body) =
        json_of(files_response(CONTRACT, &files_body("#![no_std]\n".into()))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=300"));
    assert_eq!(body["files"][0]["path"], "source/src/lib.rs");
    assert_eq!(body["files"][0]["content"], "#![no_std]\n");
}

#[tokio::test]
async fn files_too_big_for_a_lambda_response_are_refused() {
    let (status, _, body) = json_of(files_response(
        CONTRACT,
        &files_body("x".repeat(MAX_RESPONSE_BYTES)),
    ))
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "source_archive_invalid");
}

#[tokio::test]
async fn a_failed_download_is_the_service_not_the_archive() {
    use crate::runtime_enrichment::source_verification::VerificationError;
    let (status, _, body) = json_of(archive_error(
        CONTRACT,
        ArchiveError::Download(VerificationError::Status(503)),
    ))
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "source_verification_failed");
    let (_, _, body) = json_of(archive_error(CONTRACT, ArchiveError::TooLarge)).await;
    assert_eq!(body["code"], "source_archive_invalid");
}

#[tokio::test]
async fn invalid_contract_id_is_a_400() {
    let st = state(clickhouse::Client::default(), None);
    let (status, _, _) = json_of(get_verification(State(st), Path("GABC".to_owned())).await).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// A verifier's answer as the Query API sends it, for the pure helpers.
fn verifier(host: &str, status: &str, link: Option<&str>, sha: Option<&str>) -> VerifierResult {
    let mut r = json!({
        "verifier": { "url": format!("https://{host}"), "name": host },
        "status": status,
        "source_sha256": sha,
    });
    if let Some(url) = link {
        r["verified_source_code_uris"] =
            json!([{ "location": "verifier_self_storage", "url": url }]);
    }
    serde_json::from_value(r).unwrap()
}

#[test]
fn any_verifiers_copy_of_the_archive_will_do() {
    // Each verifier keeps its own copy of the same bytes; one bad copy does
    // not hide the next (live: 5 of 5 doubly-verified hashes, one sha256).
    let sha = "EC".repeat(32);
    let records = [
        verifier(
            "results.dev.example",
            "verified",
            Some("https://elsewhere.example/a.tar.gz"),
            Some(&sha),
        ),
        verifier(
            "results.staging.example",
            "verified",
            Some("https://results.staging.example/a.tar.gz"),
            Some(&sha),
        ),
    ];
    let (publisher, published) = archive_of(&records).unwrap().unwrap();
    assert_eq!(published.url, "https://results.staging.example/a.tar.gz");
    // As published: the case of the hex is the verifier's.
    assert_eq!(published.sha256, "EC".repeat(32));
    assert_eq!(published.format, "tar.gz");
    // The recipe beside it comes from the same verifier.
    assert_eq!(publisher.verifier.url, "https://results.staging.example");
}

#[test]
fn an_archive_without_its_sha256_is_an_error() {
    let records = [verifier(
        "results.staging.example",
        "verified",
        Some("https://results.staging.example/a.zip"),
        None,
    )];
    assert!(matches!(
        archive_of(&records),
        Err(ArchiveError::Corrupt(_))
    ));
}

#[test]
fn a_mismatched_verifiers_copy_is_never_offered() {
    let records = [verifier(
        "results.staging.example",
        "mismatched",
        Some("https://results.staging.example/a.zip"),
        Some(&"ab".repeat(32)),
    )];
    assert!(archive_of(&records).unwrap().is_none());
    assert!(archive_of(&[]).unwrap().is_none());
}

#[tokio::test]
async fn files_are_downloaded_checked_and_unpacked() {
    use sha2::{Digest, Sha256};
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    let body = b"#![no_std]\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    tar.append_data(&mut header, "source/src/lib.rs", &body[..])
        .unwrap();
    let bytes = tar.into_inner().unwrap().finish().unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(url_path("/sources/a.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
        .mount(&server)
        .await;
    let fetcher = source_verification::new_fetcher(&server.uri()).unwrap();
    let mut published = VerifiedSourceArchive {
        url: format!("{}/sources/a.tar.gz", server.uri()),
        sha256: hex::encode(Sha256::digest(&bytes)),
        format: "tar.gz".into(),
    };
    let files = fetch_files(&fetcher, &published).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "source/src/lib.rs");
    assert_eq!(files[0].text.as_deref(), Some("#![no_std]\n"));

    published.sha256 = "00".repeat(32);
    assert!(matches!(
        fetch_files(&fetcher, &published).await,
        Err(ArchiveError::ShaMismatch { .. })
    ));
}

fn hash() -> String {
    "aa".repeat(32)
}

/// One verifier record in the Query API's shape.
fn record(status: &str, archive: Option<&str>) -> Value {
    let mut r = json!({
        "verifier": { "url": "https://results.staging.example", "name": "SCVS Staging Verifier" },
        "status": status,
        "bldimg": "registry.example/env@sha256:2e21e7a2605cadca33f938affba710d97de0fd3d8775b0c1efdfd0b0d841997e",
        "bldopt": ["--locked", "--manifest-path=workers/dvn/Cargo.toml", "--package=dvn"],
        "source_sha256": "3ee9dbfefc5fe690b8a2bb19f495905bc1e25f98c349be816ffaab4e6073db03",
        "processed_at": "2026-10-08T02:51:00.000Z",
        "results_urls": [{ "type": "report", "url": "https://results.staging.example/reports/r.json" }]
    });
    if let Some(url) = archive {
        r["verified_source_code_uris"] =
            json!([{ "location": "verifier_self_storage", "url": url }]);
    }
    r
}

async fn query_api(agreement: &str, records: Vec<Value>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(url_path(format!("/api/query/v1/wasms/{}", hash())))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "wasm_hash": hash(),
            "updated_at": "2026-10-14T13:30:48.961Z",
            "agreement": agreement,
            "source_code_verifications": records
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn verification_reads_the_contract_hash_and_the_aggregator() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping verified-source handler check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    ch.query(&format!(
        "INSERT INTO soroban_contracts (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, \
         deployer_id, deployed_at_ledger, contract_type, is_sac) VALUES \
         (1, '{CONTRACT}', unhex('{}'), 100, NULL, 100, 1, false)",
        hash()
    ))
    .execute()
    .await
    .expect("seed contract");

    let fetcher_for = |server: &MockServer| {
        Some(source_verification::new_fetcher(&format!("{}/api/query/v1", server.uri())).unwrap())
    };

    // ---- verified, source published ----
    let archive = "https://results.staging.example/sources/3ee9dbfe.zip";
    let server = query_api(
        "unanimous_verified",
        vec![record("verified", Some(archive))],
    )
    .await;
    let st = state(ch.clone(), fetcher_for(&server));
    let (status, cache, body) =
        json_of(get_verification(State(st), Path(CONTRACT.to_owned())).await).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(cache.as_deref(), Some("public, max-age=60"));
    assert_eq!(body["wasm_hash"], hash());
    assert_eq!(body["status"], "verified");
    assert_eq!(body["checked_at"], "2026-10-14T13:30:48.961Z");
    assert_eq!(body["verifiers"][0]["name"], "SCVS Staging Verifier");
    assert_eq!(body["source"]["url"], archive);
    assert_eq!(body["source"]["format"], "zip");
    assert_eq!(body["build"]["package"], "dvn");
    assert_eq!(body["build"]["manifest_path"], "workers/dvn/Cargo.toml");
    assert_eq!(
        body["build"]["report_url"],
        "https://results.staging.example/reports/r.json"
    );

    // ---- verified, nothing published: no source, and no files endpoint ----
    let server = query_api("unanimous_verified", vec![record("verified", None)]).await;
    let st = state(ch.clone(), fetcher_for(&server));
    let (_, _, body) =
        json_of(get_verification(State(st.clone()), Path(CONTRACT.to_owned())).await).await;
    assert_eq!(body["status"], "verified_private");
    assert_eq!(body["source"], Value::Null);
    let (status, _, _) =
        json_of(get_verified_source(State(st), Path(CONTRACT.to_owned())).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ---- submitted source that did not verify is not served ----
    for agreement in ["single", "disputed"] {
        let records = if agreement == "single" {
            vec![record("mismatched", Some(archive))]
        } else {
            vec![
                record("mismatched", Some(archive)),
                record("verified", Some(archive)),
            ]
        };
        let server = query_api(agreement, records).await;
        let st = state(ch.clone(), fetcher_for(&server));
        let (_, _, body) =
            json_of(get_verification(State(st.clone()), Path(CONTRACT.to_owned())).await).await;
        assert_ne!(body["status"], "verified", "{agreement}: {body}");
        assert_eq!(
            body["source"],
            Value::Null,
            "{agreement}: no submitted source offered"
        );
        assert_eq!(
            body["build"],
            Value::Null,
            "{agreement}: no recipe for unverified code"
        );
        let (status, _, _) =
            json_of(get_verified_source(State(st), Path(CONTRACT.to_owned())).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{agreement}");
    }

    // ---- an archive off the verifier's host is refused, not fetched ----
    let server = query_api(
        "single",
        vec![record(
            "verified",
            Some("https://elsewhere.example/sources/3ee9dbfe.zip"),
        )],
    )
    .await;
    let st = state(ch.clone(), fetcher_for(&server));
    let (status, _, body) =
        json_of(get_verified_source(State(st), Path(CONTRACT.to_owned())).await).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "source_archive_invalid");

    // ---- the aggregator down: a loud 500, never "not verified" ----
    let down = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&down)
        .await;
    let st = state(ch.clone(), fetcher_for(&down));
    let (status, _, body) =
        json_of(get_verification(State(st), Path(CONTRACT.to_owned())).await).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "source_verification_failed");

    // ---- an unknown contract never reaches the aggregator ----
    let st = state(ch.clone(), fetcher_for(&down));
    let (status, _, _) = json_of(
        get_verification(
            State(st),
            Path("CA22CTI77LDCCJZJSTGFPJVH7Q5BEAUPLPQW3XFXQOSKI7O3I2VRU6QN".to_owned()),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop");
}
