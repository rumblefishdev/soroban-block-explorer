use super::*;

#[test]
fn reads_a_trimmed_comma_separated_pool() {
    let urls = parse_rpc_urls(Some(" https://a.example , https://b.example/,, ")).unwrap();
    assert_eq!(urls, vec!["https://a.example", "https://b.example/"]);
}

#[test]
fn unset_is_an_error_naming_the_variable() {
    let err = parse_rpc_urls(None).unwrap_err();
    assert!(err.starts_with("SOROBAN_RPC_URLS is not set"), "{err}");
}

#[test]
fn a_list_without_urls_is_an_error_too() {
    assert!(parse_rpc_urls(Some("")).is_err());
    assert!(parse_rpc_urls(Some(" , ,")).is_err());
}
