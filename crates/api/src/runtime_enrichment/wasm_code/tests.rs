use super::*;

#[test]
fn counts_markers_in_emitted_rust() {
    let src = r#"
        pub fn transfer(env: Env) -> i128 {
            let x = get(&todo!("unknown value")).unwrap();
            let y = todo !("host call");
            var_1 + var_2 + var_1
        }
        pub fn balance(env: Env) -> i128 { var_10 }
    "#;
    let c = MarkerCounts::of(src);
    assert_eq!(c.functions, 2);
    assert_eq!(c.todo_holes, 2);
    assert_eq!(c.unknown_vars, 3); // var_1, var_2, var_10 — deduped
}

#[test]
fn var_matching_requires_identifier_boundaries() {
    // `my_var_3` is part of a longer identifier; `var_` with no digits
    // and `var_x` are not markers.
    let src = "my_var_3 var_ var_x var_7";
    let c = MarkerCounts::of(src);
    assert_eq!(c.unknown_vars, 1); // only var_7
}

#[test]
fn decompiles_a_trivial_wasm_to_rust() {
    // Smallest valid wasm module: magic + version. `DecompileMode::Auto`
    // falls back to generic-wasm decompilation, so even a spec-less
    // module takes the Rust path (with zero functions).
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let d = decompile_blocking(&wasm, false).expect("rust via generic mode");
    assert_eq!(d.representation, "rust");
    assert!(d.rust_error.is_none());
    assert_eq!(d.functions, Some(0));
}

#[test]
fn garbage_bytes_fail_both_paths() {
    let not_wasm = b"definitely not a wasm module";
    assert!(decompile_blocking(not_wasm, false).is_err());
    assert!(decompile_blocking(not_wasm, true).is_err());
}

/// Live-RPC smoke test (run explicitly: `cargo test -- --ignored`).
/// The hash is the most-instantiated mainnet contract (task 0465 sweep);
/// size asserted against the bytes fetched during the sweep.
#[tokio::test]
#[ignore = "hits live mainnet RPC"]
async fn fetches_real_wasm_by_hash() {
    let fetcher = WasmCodeFetcher::new().expect("build fetcher");
    let code = fetcher
        .fetch_wasm("07097f83dae3b746db7dba3263d9cc334efb88a9a7d5450fb96ca19f33d284b0")
        .await
        .expect("rpc ok")
        .expect("entry live");
    assert_eq!(code.len(), 6831);
}

#[test]
fn wat_direct_request() {
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let d = decompile_blocking(&wasm, true).expect("wat");
    assert_eq!(d.representation, "wat");
    assert!(d.rust_error.is_none());
}

// ---- RPC pool failover, against local endpoints --------------------------

const HASH: [u8; 32] = [7; 32];

/// A `getLedgerEntries` answer holding one CONTRACT_CODE entry for `HASH`.
fn code_answer() -> serde_json::Value {
    let entry = LedgerEntryData::ContractCode(stellar_xdr::ContractCodeEntry {
        ext: stellar_xdr::ContractCodeEntryExt::V0,
        hash: Hash(HASH),
        code: b"\0asm".to_vec().try_into().unwrap(),
    });
    let xdr = BASE64.encode(entry.to_xdr(Limits::none()).unwrap());
    serde_json::json!({ "jsonrpc": "2.0", "id": 1,
        "result": { "entries": [ { "xdr": xdr } ], "latestLedger": 1 } })
}

fn empty_answer() -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1,
        "result": { "entries": [], "latestLedger": 1 } })
}

/// Serves each path with a fixed (status, body); returns the base URL.
async fn serve(routes: Vec<(&'static str, u16, serde_json::Value)>) -> String {
    let mut app = axum::Router::new();
    for (path, status, body) in routes {
        app = app.route(
            path,
            axum::routing::post(move || {
                let body = body.clone();
                async move {
                    (
                        axum::http::StatusCode::from_u16(status).unwrap(),
                        axum::Json(body),
                    )
                }
            }),
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A failing endpoint is skipped; the next one's entry is returned.
#[tokio::test]
async fn failover_returns_the_next_endpoints_entry() {
    let base = serve(vec![
        ("/down", 500, empty_answer()),
        ("/up", 200, code_answer()),
    ])
    .await;
    let fetcher = WasmCodeFetcher::with_urls(vec![format!("{base}/down"), format!("{base}/up")]);
    let code = fetcher.fetch_wasm(&hex::encode(HASH)).await.unwrap();
    assert_eq!(code.as_deref(), Some(&b"\0asm"[..]));
}

/// An empty answer is not believed until every endpoint gives it.
#[tokio::test]
async fn empty_everywhere_is_not_live_empty_once_asks_on() {
    let base = serve(vec![
        ("/a", 200, empty_answer()),
        ("/b", 200, code_answer()),
    ])
    .await;
    let mixed = WasmCodeFetcher::with_urls(vec![format!("{base}/a"), format!("{base}/b")]);
    assert!(
        mixed
            .fetch_wasm(&hex::encode(HASH))
            .await
            .unwrap()
            .is_some()
    );

    let all_empty = WasmCodeFetcher::with_urls(vec![format!("{base}/a"), format!("{base}/a")]);
    assert!(
        all_empty
            .fetch_wasm(&hex::encode(HASH))
            .await
            .unwrap()
            .is_none()
    );
}

/// Empty on one endpoint and a failure on another is an error, not "gone".
#[tokio::test]
async fn empty_plus_failure_is_an_error() {
    let base = serve(vec![
        ("/a", 200, empty_answer()),
        ("/down", 503, empty_answer()),
    ])
    .await;
    let fetcher = WasmCodeFetcher::with_urls(vec![format!("{base}/a"), format!("{base}/down")]);
    assert!(matches!(
        fetcher.fetch_wasm(&hex::encode(HASH)).await,
        Err(FetchError::Rpc(_))
    ));
}

/// An RPC error object stops at once, without asking the rest of the pool.
#[tokio::test]
async fn rpc_error_object_stops_at_once() {
    let err = serde_json::json!({ "jsonrpc": "2.0", "id": 1,
        "error": { "code": -32600, "message": "bad" } });
    let base = serve(vec![("/err", 200, err), ("/up", 200, code_answer())]).await;
    let fetcher = WasmCodeFetcher::with_urls(vec![format!("{base}/err"), format!("{base}/up")]);
    assert!(matches!(
        fetcher.fetch_wasm(&hex::encode(HASH)).await,
        Err(FetchError::RpcError(_))
    ));
}
