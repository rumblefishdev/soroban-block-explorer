use super::check_network;
use xdr_parser::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

const RPC: &str = "https://rpc.example";

#[test]
fn testnet_rpc_into_testnet_database_runs() {
    assert_eq!(
        check_network(RPC, TESTNET_PASSPHRASE, TESTNET_PASSPHRASE, "testnet"),
        Ok(())
    );
}

#[test]
fn testnet_rpc_with_database_unset_is_refused() {
    // `database_from_env` resolves an unset CLICKHOUSE_DATABASE to `default`.
    let unset = db_clickhouse::PROD_DATABASE;
    let err = check_network(RPC, TESTNET_PASSPHRASE, TESTNET_PASSPHRASE, unset).unwrap_err();
    assert!(
        err.contains("database `default` holds the network"),
        "{err}"
    );
}

#[test]
fn testnet_rpc_into_default_database_is_refused() {
    assert!(check_network(RPC, TESTNET_PASSPHRASE, TESTNET_PASSPHRASE, "default").is_err());
}

#[test]
fn mainnet_rpc_into_default_database_runs() {
    assert_eq!(
        check_network(RPC, MAINNET_PASSPHRASE, MAINNET_PASSPHRASE, "default"),
        Ok(())
    );
}

#[test]
fn rpc_of_another_network_than_the_passphrase_is_refused() {
    let err = check_network(RPC, TESTNET_PASSPHRASE, MAINNET_PASSPHRASE, "default").unwrap_err();
    assert!(
        err.starts_with("RPC https://rpc.example serves the network"),
        "{err}"
    );
}

#[test]
fn unknown_database_is_refused() {
    let err = check_network(RPC, TESTNET_PASSPHRASE, TESTNET_PASSPHRASE, "scratch").unwrap_err();
    assert!(err.contains("belongs to no known network"), "{err}");
}
