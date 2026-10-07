use super::*;

#[test]
fn resolve_ipfs_swaps_scheme() {
    assert_eq!(
        resolve_ipfs_to_https("ipfs://QmFoo/1.json"),
        "https://ipfs.io/ipfs/QmFoo/1.json"
    );
    assert_eq!(
        resolve_ipfs_to_https("https://example.com/1.json"),
        "https://example.com/1.json"
    );
}

#[test]
fn host_of_extracts_authority() {
    assert_eq!(
        host_of("https://example.com/path"),
        Some("example.com".into())
    );
    assert_eq!(
        host_of("https://example.com:8443/x"),
        Some("example.com".into())
    );
    assert_eq!(host_of("ipfs://Qm.../path"), None);
    assert_eq!(host_of("garbage"), None);
}

#[test]
fn build_envelope_roundtrip_decodes_token_id() {
    // Verify the envelope builder produces XDR that decodes back to
    // the same InvokeContract args. Uses a synthetic strkey so the
    // test doesn't depend on any live contract id.
    let contract = stellar_strkey::Contract([0xAB; 32]).to_string();
    let envelope_b64 =
        build_simulate_envelope(&contract, TOKEN_URI_FN, Some(4521)).expect("build ok");
    let raw = BASE64.decode(&envelope_b64).expect("base64 decode");
    let env = TransactionEnvelope::from_xdr(&raw, Limits::none()).expect("xdr roundtrip");
    let TransactionEnvelope::Tx(v1) = env else {
        panic!("expected V1 envelope");
    };
    let op = v1.tx.operations.first().expect("one op");
    let OperationBody::InvokeHostFunction(invoke) = &op.body else {
        panic!("expected InvokeHostFunction");
    };
    let HostFunction::InvokeContract(args) = &invoke.host_function else {
        panic!("expected InvokeContract");
    };
    assert_eq!(args.args.len(), 1);
    match &args.args[0] {
        ScVal::U32(n) => assert_eq!(*n, 4521),
        other => panic!("expected ScVal::U32, got {other:?}"),
    }
    assert_eq!(args.function_name.0.as_slice(), TOKEN_URI_FN.as_bytes());
}

#[test]
fn build_envelope_for_name_is_zero_arg() {
    // The SEP-50 `name()` envelope (task 0340): right function symbol,
    // no arguments.
    let contract = stellar_strkey::Contract([0xAB; 32]).to_string();
    let envelope_b64 = build_simulate_envelope(&contract, NAME_FN, None).expect("build ok");
    let raw = BASE64.decode(&envelope_b64).expect("base64 decode");
    let env = TransactionEnvelope::from_xdr(&raw, Limits::none()).expect("xdr roundtrip");
    let TransactionEnvelope::Tx(v1) = env else {
        panic!("expected V1 envelope");
    };
    let op = v1.tx.operations.first().expect("one op");
    let OperationBody::InvokeHostFunction(invoke) = &op.body else {
        panic!("expected InvokeHostFunction");
    };
    let HostFunction::InvokeContract(args) = &invoke.host_function else {
        panic!("expected InvokeContract");
    };
    assert!(args.args.is_empty());
    assert_eq!(args.function_name.0.as_slice(), NAME_FN.as_bytes());
}

#[test]
fn usable_collection_name_trims_and_bounds() {
    assert_eq!(usable_collection_name("  Punks  "), Some("Punks".into()));
    assert_eq!(usable_collection_name(""), None);
    assert_eq!(usable_collection_name("   "), None);
    assert_eq!(
        usable_collection_name(&"x".repeat(MAX_COLLECTION_NAME_CHARS + 1)),
        None
    );
}

#[test]
fn decode_result_handles_scval_string() {
    let uri = b"ipfs://QmTest/4521.json";
    let scval = ScVal::String(ScString(StringM::try_from(uri.to_vec()).unwrap()));
    let b64 = BASE64.encode(scval.to_xdr(Limits::none()).unwrap());
    assert_eq!(
        decode_string_result(&b64, TOKEN_URI_FN).unwrap(),
        "ipfs://QmTest/4521.json"
    );
}

#[test]
fn decode_result_rejects_non_string_scval() {
    let scval = ScVal::U32(42);
    let b64 = BASE64.encode(scval.to_xdr(Limits::none()).unwrap());
    assert!(matches!(
        decode_string_result(&b64, TOKEN_URI_FN),
        Err(NftTokenUriError::MalformedRpcResponse(_))
    ));
}

// wiremock-driven JSON-RPC tests. End-to-end metadata-URL fetch
// not wiremocked: `validate_uri` rejects loopback hosts on purpose.

use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn scval_string_b64(uri: &str) -> String {
    let scval = ScVal::String(ScString(
        StringM::try_from(uri.as_bytes().to_vec()).unwrap(),
    ));
    BASE64.encode(scval.to_xdr(Limits::none()).unwrap())
}

/// A non-String `name()` retval — exercises the "decode fails → permanent,
/// fold to Ok(None)" path.
fn scval_u32_b64(n: u32) -> String {
    BASE64.encode(ScVal::U32(n).to_xdr(Limits::none()).unwrap())
}

#[tokio::test]
async fn simulate_transaction_happy_path() {
    let mock = MockServer::start().await;
    let xdr_b64 = scval_string_b64("ipfs://QmDeadBeef/4521.json");

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(serde_json::json!({
            "method": "simulateTransaction",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "minResourceFee": "0",
                "results": [{ "auth": [], "xdr": xdr_b64 }],
            },
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let envelope = "AAAA".to_owned(); // body content irrelevant; mock matches by method
    let got = simulate_transaction(&client, &mock.uri(), &envelope)
        .await
        .expect("happy path");
    // RPC layer returns the raw xdr_b64; ScVal decode is a separate fn.
    assert_eq!(
        decode_string_result(&got, TOKEN_URI_FN).unwrap(),
        "ipfs://QmDeadBeef/4521.json"
    );
}

#[tokio::test]
async fn resolve_collection_name_happy_path_hits_rpc_once() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(serde_json::json!({
            "method": "simulateTransaction",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "minResourceFee": "0",
                "results": [{ "auth": [], "xdr": scval_string_b64("  Cool Punks  ") }],
            },
        })))
        // The second resolve MUST come from the per-contract cache.
        .expect(1)
        .mount(&mock)
        .await;

    let fetcher = NftTokenUriFetcher::with_rpc_url(mock.uri()).expect("build fetcher");
    let contract = stellar_strkey::Contract([0xCD; 32]).to_string();
    for _ in 0..2 {
        let got = fetcher
            .resolve_collection_name(&contract)
            .await
            .expect("name() resolves");
        // Trimmed on the way in.
        assert_eq!(got.as_deref(), Some("Cool Punks"));
    }
}

#[tokio::test]
async fn resolve_collection_name_permanent_fail_is_cached_none() {
    // A contract without `name()` answers HTTP 200 with a contract-level
    // error → SorobanRpc → PERMANENT → folded to Ok(None) and CACHED, so a
    // big collection costs one RPC per run, not one per token.
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "error": "host fn error: missing entry",
            },
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let fetcher = NftTokenUriFetcher::with_rpc_url(mock.uri()).expect("build fetcher");
    let contract = stellar_strkey::Contract([0xEF; 32]).to_string();
    for _ in 0..2 {
        let got = fetcher
            .resolve_collection_name(&contract)
            .await
            .expect("permanent fail folds to Ok");
        assert_eq!(got, None);
    }
}

#[tokio::test]
async fn resolve_collection_name_non_string_return_folds_to_none() {
    // A contract whose name() returns a non-String ScVal (here U32): the
    // simulate SUCCEEDS but the retval does not decode to a String. That is
    // a PERMANENT contract fact — it must fold to a cached Ok(None), never
    // surface as an Err (every caller treats Err as transient → a retry
    // storm on every token / every run). `.expect(1)` proves the fold is
    // cached (the 2nd resolve is a cache hit).
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "minResourceFee": "0",
                "results": [{ "auth": [], "xdr": scval_u32_b64(42) }],
            },
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let fetcher = NftTokenUriFetcher::with_rpc_url(mock.uri()).expect("build fetcher");
    let contract = stellar_strkey::Contract([0xAB; 32]).to_string();
    for _ in 0..2 {
        let got = fetcher
            .resolve_collection_name(&contract)
            .await
            .expect("non-String name() folds to Ok(None), never Err");
        assert_eq!(got, None);
    }
}

#[tokio::test]
async fn resolve_collection_name_transient_fail_is_err_and_uncached() {
    // 503 from every pool endpoint → transient → Err (NOT cached: a later
    // call re-asks the RPC instead of cementing the outage for the TTL).
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&mock)
        .await;

    let fetcher = NftTokenUriFetcher::with_rpc_url(mock.uri()).expect("build fetcher");
    let contract = stellar_strkey::Contract([0x12; 32]).to_string();
    for _ in 0..2 {
        let err = fetcher
            .resolve_collection_name(&contract)
            .await
            .expect_err("503 is transient → Err");
        assert!(super::super::errors::is_transient(&err));
    }
}

#[tokio::test]
async fn simulate_transaction_jsonrpc_error() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32600, "message": "invalid request" },
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let err = simulate_transaction(&client, &mock.uri(), "AAAA")
        .await
        .expect_err("JSON-RPC error must propagate");
    assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
}

#[tokio::test]
async fn simulate_transaction_contract_revert() {
    // RPC server returns 200 but result.error indicates a contract revert.
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "error": "host fn error: missing entry",
            },
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let err = simulate_transaction(&client, &mock.uri(), "AAAA")
        .await
        .expect_err("contract-side error must propagate");
    assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
}

#[tokio::test]
async fn simulate_transaction_missing_results_array() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "latestLedger": 100 },
        })))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let err = simulate_transaction(&client, &mock.uri(), "AAAA")
        .await
        .expect_err("malformed response must surface");
    assert!(matches!(err, NftTokenUriError::MalformedRpcResponse(_)));
}

#[tokio::test]
async fn simulate_transaction_5xx_is_http_error() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let err = simulate_transaction(&client, &mock.uri(), "AAAA")
        .await
        .expect_err("5xx must surface as Http");
    let NftTokenUriError::Http { source, .. } = &err else {
        panic!("expected Http variant, got {err:?}");
    };
    assert_eq!(source.status().map(|s| s.as_u16()), Some(503));
    assert!(super::super::errors::is_transient(&err));
}

#[tokio::test]
async fn resolve_propagates_permanent_error_as_err() {
    // Permanent fails (MalformedInput here) must surface as Err so
    // the worker call site can warn-log every occurrence. `moka`'s
    // `try_get_with` does not cache Err, so a repeat call re-enters
    // `fetch_uncached` — observability + self-healing over the
    // sub-ms cache-hit savings of a negative cache.
    let fetcher = NftTokenUriFetcher::with_rpc_url("http://unused".to_owned()).expect("build");
    let err = fetcher
        .resolve("not-a-strkey", "42")
        .await
        .expect_err("permanent fail must propagate as Err");
    assert!(matches!(*err, NftTokenUriError::MalformedInput { .. }));
    // Repeat call: must also propagate Err (not silently cached).
    let err2 = fetcher
        .resolve("not-a-strkey", "42")
        .await
        .expect_err("repeat permanent fail must still propagate");
    assert!(matches!(*err2, NftTokenUriError::MalformedInput { .. }));
}

#[tokio::test]
async fn fetch_uncached_rejects_non_u32_token_id() {
    // Pure structural check — `fetch_uncached` short-circuits before
    // any network call when token_id isn't a u32.
    let client = reqwest::Client::new();
    let err = super::fetch_uncached(
        &client,
        &["http://unused".to_owned()],
        &["https://gw/ipfs/".to_owned()],
        0,
        "C...",
        "not-a-number",
    )
    .await
    .expect_err("non-u32 token_id must hard-fail");
    assert!(matches!(
        err,
        NftTokenUriError::MalformedInput { field, .. } if field.contains("token_id")
    ));
}

#[tokio::test]
async fn fetch_uncached_rejects_bad_contract_strkey() {
    let client = reqwest::Client::new();
    let err = super::fetch_uncached(
        &client,
        &["http://unused".to_owned()],
        &["https://gw/ipfs/".to_owned()],
        0,
        "not-a-strkey",
        "42",
    )
    .await
    .expect_err("malformed contract strkey must hard-fail");
    assert!(matches!(
        err,
        NftTokenUriError::MalformedInput { field, .. } if field.contains("contract_id")
    ));
}

// ---- task 0311: multi-provider RPC rotation + failover ----

#[tokio::test]
async fn simulate_failover_advances_past_429() {
    // First RPC 429s; the pool must fail over to the healthy second.
    let limited = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&limited)
        .await;
    let healthy = MockServer::start().await;
    let xdr = scval_string_b64("ipfs://QmGood/1.json");
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "latestLedger": 1, "results": [{ "auth": [], "xdr": xdr }] },
        })))
        .mount(&healthy)
        .await;

    let client = reqwest::Client::new();
    let contract = stellar_strkey::Contract([0xCD; 32]).to_string();
    // start=0 → tries `limited` (429) first, fails over to `healthy`.
    let got =
        super::simulate_with_failover(&client, &[limited.uri(), healthy.uri()], 0, &contract, 1)
            .await
            .expect("must fail over from 429 to the healthy RPC");
    assert_eq!(
        decode_string_result(&got, TOKEN_URI_FN).unwrap(),
        "ipfs://QmGood/1.json"
    );
}

#[tokio::test]
async fn simulate_failover_all_429_surfaces_transient() {
    // Whole pool 429s → the exhausted error must classify transient (so the
    // worker requests an SQS retry rather than burning a permanent sentinel).
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    for m in [&a, &b] {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429))
            .mount(m)
            .await;
    }
    let client = reqwest::Client::new();
    let contract = stellar_strkey::Contract([0x01; 32]).to_string();
    let err = super::simulate_with_failover(&client, &[a.uri(), b.uri()], 0, &contract, 1)
        .await
        .expect_err("a fully-429 pool must surface an error");
    assert!(
        super::super::errors::is_transient(&err),
        "exhausted-429 pool should be transient, got {err:?}"
    );
}

#[tokio::test]
async fn simulate_failover_stops_on_deterministic_error() {
    // A contract-side revert is identical on every endpoint → must NOT fail
    // over (would waste the whole pool on a permanent fault).
    let first = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "latestLedger": 1, "error": "HostError: Error(Contract, #5)" },
        })))
        .mount(&first)
        .await;
    let never = MockServer::start().await;
    let xdr = scval_string_b64("ipfs://QmNever/1.json");
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "latestLedger": 1, "results": [{ "auth": [], "xdr": xdr }] },
        })))
        .mount(&never)
        .await;

    let client = reqwest::Client::new();
    let contract = stellar_strkey::Contract([0x02; 32]).to_string();
    let err = super::simulate_with_failover(&client, &[first.uri(), never.uri()], 0, &contract, 1)
        .await
        .expect_err("deterministic contract error must not fail over");
    assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
}

#[tokio::test]
async fn metadata_3xx_surfaces_not_panics() {
    // A 3xx from a gateway must surface as a failover-worthy error WITHOUT
    // panicking (the old `error_for_status().expect_err()` panicked on 3xx).
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(301))
        .mount(&mock)
        .await;
    // The PRODUCTION policy (same-eTLD+1) must stop this hop: wiremock's
    // host is an IP literal, which `validate_host` rejects, so the 301 is
    // returned as a response rather than followed — same observable
    // behaviour `Policy::limited(0)` had here before lore-0455.
    let client = reqwest::Client::builder()
        .redirect(crate::sep1::same_etld1_redirect_policy())
        .build()
        .unwrap();
    let err = super::fetch_one_metadata(&client, &format!("{}/x", mock.uri()))
        .await
        .expect_err("3xx must surface as error, not panic");
    // Reaching here = no panic. The key property: a 3xx gateway is
    // failover-worthy so the pool advances to the next one.
    assert!(super::super::errors::is_endpoint_fault(&err), "got {err:?}");
}

/// The case `Policy::limited(0)` used to lose (lore-0455, measured
/// 2026-08-18): a directory CID without a trailing slash answers `301`
/// to the SAME host plus `/`. The shared same-eTLD+1 policy must follow
/// that hop. Wiremock binds to an IP literal (which the policy rejects
/// by design), so this pins the decision function directly on the
/// hostname shapes from the measurement.
#[test]
fn trailing_slash_redirect_is_followed_and_offhost_is_not() {
    use crate::sep1::redirect_decisions_for_test as allowed;
    // ipfs.io/ipfs/<dirCID> -> ipfs.io/ipfs/<dirCID>/ : same host, follow.
    assert!(allowed("ipfs.io", "ipfs.io"));
    // gateway.pinata.cloud -> www.pinata.cloud : same eTLD+1, follow.
    assert!(allowed("gateway.pinata.cloud", "www.pinata.cloud"));
    // dweb.link-style per-CID subdomain hop: PSL-listed gateway domains
    // resolve to different registrable domains, so it must be stopped.
    assert!(!allowed("dweb.link", "bafybeigdyrzt.ipfs.dweb.link"));
    // Plain off-host is stopped.
    assert!(!allowed("ipfs.io", "evil.example"));
    // IP-literal target is stopped (SSRF gate).
    assert!(!allowed("ipfs.io", "169.254.169.254"));
}

#[test]
fn ipfs_candidates_rotate_and_passthrough() {
    let gws = vec![
        "https://gw-a/ipfs/".to_owned(),
        "https://gw-b/ipfs/".to_owned(),
    ];
    // ipfs:// → one URL per gateway, order rotated by `start`.
    assert_eq!(
        super::ipfs_candidate_urls("ipfs://QmX/1.json", &gws, 0),
        vec![
            "https://gw-a/ipfs/QmX/1.json",
            "https://gw-b/ipfs/QmX/1.json"
        ]
    );
    assert_eq!(
        super::ipfs_candidate_urls("ipfs://QmX/1.json", &gws, 1),
        vec![
            "https://gw-b/ipfs/QmX/1.json",
            "https://gw-a/ipfs/QmX/1.json"
        ]
    );
    // https:// → single passthrough, no rotation.
    assert_eq!(
        super::ipfs_candidate_urls("https://host.example/1.json", &gws, 0),
        vec!["https://host.example/1.json"]
    );
}

// Live mainnet smoke. Default-ignored — hits SDF public RPC. Run:
//   cargo test -p enrichment-shared --lib live_mainnet -- --ignored --nocapture

const LIVE_RPC_URL: &str = "https://mainnet.sorobanrpc.com";
const LIVE_NFT_CONTRACT: &str = "CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY";

/// 0-arg variant: JamesBachini's tutorial contract defines
/// `token_uri(env) -> String` (no caller args). The default builder
/// sends 1 arg per ERC-721, so we craft a 0-arg envelope here.
fn build_zero_arg_envelope(contract_id: &str) -> Result<String, NftTokenUriError> {
    let contract = stellar_strkey::Contract::from_string(contract_id).map_err(|_| {
        NftTokenUriError::MalformedInput {
            field: "contract_id strkey",
            value: contract_id.to_owned(),
        }
    })?;
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: ScAddress::Contract(ContractId(Hash(contract.0))),
                function_name: ScSymbol(StringM::try_from(TOKEN_URI_FN.as_bytes().to_vec())?),
                args: VecM::default(),
            }),
            auth: VecM::default(),
        }),
    };
    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256([0u8; 32])),
        fee: 100,
        seq_num: SequenceNumber(0),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![op].try_into()?,
        ext: TransactionExt::V0,
    };
    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });
    Ok(BASE64.encode(envelope.to_xdr(Limits::none())?))
}

#[tokio::test]
#[ignore = "hits live SDF mainnet RPC; run with --ignored"]
async fn live_mainnet_zero_arg_token_uri_success() {
    let client = reqwest::Client::new();
    let envelope = build_zero_arg_envelope(LIVE_NFT_CONTRACT).expect("envelope");
    let xdr_b64 = simulate_transaction(&client, LIVE_RPC_URL, &envelope)
        .await
        .expect("RPC call must succeed for known-good 0-arg token_uri");
    let uri = decode_string_result(&xdr_b64, TOKEN_URI_FN).expect("ScVal::String decode");
    eprintln!("✓ live mainnet token_uri returned: {uri}");
    assert!(!uri.is_empty(), "URI must be non-empty");
    // Only the schemes `validate_uri` actually accepts —
    // tightened from a wider list that briefly allowed bare CID
    // prefixes (`Qm…`, `bafy…`) which would never pass production
    // validation.
    assert!(
        uri.starts_with("https://") || uri.starts_with("ipfs://"),
        "URI scheme not recognised: {uri}"
    );
}
