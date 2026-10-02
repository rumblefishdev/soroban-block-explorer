use super::*;
use xdr_parser::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

const TESTNET_FOLDER: &str = "v1.1/stellar/ledgers/testnet/2025-12-18";
const OWN_BUCKET: &str = "production-stellar-ledger-data";

#[test]
fn our_own_bucket_reads_its_root() {
    let source = LedgerSource::new(OWN_BUCKET, None, MAINNET_PASSPHRASE).unwrap();
    assert_eq!(
        source,
        LedgerSource::OwnBucket {
            bucket: OWN_BUCKET.to_string()
        }
    );
    assert_eq!(source.bucket(), OWN_BUCKET);
    assert_eq!(source.key_prefix(), "");
    // An empty mainnet database is seeded by a backfill first.
    assert_eq!(source.first_ledger(), None);
}

#[test]
fn our_own_bucket_is_mainnet_only() {
    let err = LedgerSource::new(OWN_BUCKET, None, TESTNET_PASSPHRASE).unwrap_err();
    assert!(err.contains("pubnet"), "names the network: {err}");
    assert!(LedgerSource::new(OWN_BUCKET, None, "Public Global Stellar Network").is_err());
}

#[test]
fn the_lake_reads_the_network_folder() {
    let source = LedgerSource::new(
        PUBLIC_BUCKET,
        Some(TESTNET_FOLDER.to_string()),
        TESTNET_PASSPHRASE,
    )
    .unwrap();
    assert_eq!(source.bucket(), PUBLIC_BUCKET);
    assert_eq!(source.key_prefix(), format!("{TESTNET_FOLDER}/"));
    // An empty testnet database reads the lake from the first closed ledger.
    assert_eq!(source.first_ledger(), Some(2));
}

#[test]
fn the_lake_without_a_folder_reads_pubnet() {
    let source = LedgerSource::new(PUBLIC_BUCKET, None, MAINNET_PASSPHRASE).unwrap();
    assert_eq!(source.key_prefix(), format!("{PUBNET_PREFIX}/"));
}

#[test]
fn a_lake_folder_of_another_network_is_refused() {
    let err = LedgerSource::new(
        PUBLIC_BUCKET,
        Some(TESTNET_FOLDER.to_string()),
        MAINNET_PASSPHRASE,
    )
    .unwrap_err();
    assert!(err.contains(TESTNET_FOLDER), "names the folder: {err}");
}

#[test]
fn a_folder_next_to_our_own_bucket_is_refused() {
    let err = LedgerSource::new(
        OWN_BUCKET,
        Some(TESTNET_FOLDER.to_string()),
        MAINNET_PASSPHRASE,
    )
    .unwrap_err();
    assert!(err.contains(TESTNET_FOLDER), "names the folder: {err}");
    assert!(err.contains(OWN_BUCKET), "names the bucket: {err}");
}

#[test]
fn a_missing_bucket_is_refused() {
    assert!(LedgerSource::new("", None, MAINNET_PASSPHRASE).is_err());
}
