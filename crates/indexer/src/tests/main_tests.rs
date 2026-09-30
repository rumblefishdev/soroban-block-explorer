use super::*;
use xdr_parser::public_archive::PUBLIC_BUCKET;

const TESTNET_FOLDER: &str = "v1.1/stellar/ledgers/testnet/2025-12-18";
const OWN_BUCKET: &str = "production-stellar-ledger-data";

#[test]
fn a_folder_in_the_data_lake_is_accepted() {
    assert!(check_folder_needs_lake(PUBLIC_BUCKET, Some(TESTNET_FOLDER)).is_ok());
}

#[test]
fn our_own_bucket_without_a_folder_is_accepted() {
    assert!(check_folder_needs_lake(OWN_BUCKET, None).is_ok());
}

#[test]
fn a_folder_next_to_our_own_bucket_is_refused() {
    let err = check_folder_needs_lake(OWN_BUCKET, Some(TESTNET_FOLDER)).unwrap_err();
    assert!(err.contains(TESTNET_FOLDER), "names the folder: {err}");
    assert!(err.contains(OWN_BUCKET), "names the bucket: {err}");
}

#[test]
fn the_data_lake_without_a_folder_is_accepted() {
    // The folder then defaults to pubnet (`public_archive_prefix`).
    assert!(check_folder_needs_lake(PUBLIC_BUCKET, None).is_ok());
}
