use super::*;
use crate::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

#[test]
fn the_pubnet_folder_belongs_to_the_mainnet_passphrase() {
    assert!(check_archive_network(PUBNET_PREFIX, MAINNET_PASSPHRASE).is_ok());
    assert!(check_archive_network(PUBNET_PREFIX, TESTNET_PASSPHRASE).is_err());
}

#[test]
fn a_testnet_genesis_folder_belongs_to_the_testnet_passphrase() {
    let testnet = "v1.1/stellar/ledgers/testnet/2025-12-18";
    assert!(check_archive_network(testnet, TESTNET_PASSPHRASE).is_ok());
    assert!(check_archive_network(testnet, MAINNET_PASSPHRASE).is_err());
}

#[test]
fn a_folder_naming_no_known_network_is_refused() {
    assert!(check_archive_network("v1.1/stellar/ledgers/futurenet", MAINNET_PASSPHRASE).is_err());
    assert!(check_archive_network("", MAINNET_PASSPHRASE).is_err());
}
