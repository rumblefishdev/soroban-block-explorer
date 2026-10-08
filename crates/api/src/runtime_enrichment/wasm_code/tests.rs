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
    let fetcher = WasmCodeFetcher::with_rpc_urls(vec!["https://mainnet.sorobanrpc.com".to_owned()])
        .expect("build fetcher");
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
