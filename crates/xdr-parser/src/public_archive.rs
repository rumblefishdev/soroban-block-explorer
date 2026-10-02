//! Where the public Stellar ledger data lake keeps each network's files, and
//! the guard that a configured folder matches the network being parsed.
//!
//! `s3://aws-public-blockchain/v1.1/stellar/ledgers/` holds `pubnet/` and one
//! `testnet/<genesis-date>/` folder per testnet reset (lore-0553). A folder of
//! the other network parses without error — ledger meta carries no network —
//! so a mismatch would silently hash every transaction wrong. Hence the check.

use crate::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

/// The public ledger dataset. World-readable: read it unsigned — a request
/// signed by a role that holds no grant on it is refused.
pub const PUBLIC_BUCKET: &str = "aws-public-blockchain";
pub const PUBLIC_BUCKET_REGION: &str = "us-east-2";

/// Mainnet's folder inside [`PUBLIC_BUCKET`].
pub const PUBNET_PREFIX: &str = "v1.1/stellar/ledgers/pubnet";

/// Ledgers 0 and 1 have no close meta on any network, so no archive holds them.
pub const FIRST_CLOSED_LEDGER: u32 = 2;

/// `PUBLIC_ARCHIVE_PREFIX` when it names a folder; `None` when unset or blank.
/// No trailing slash.
pub fn configured_archive_prefix() -> Option<String> {
    std::env::var("PUBLIC_ARCHIVE_PREFIX")
        .ok()
        .map(|p| p.trim().trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty())
}

/// The configured folder: `PUBLIC_ARCHIVE_PREFIX`, else [`PUBNET_PREFIX`].
/// No trailing slash.
pub fn public_archive_prefix() -> String {
    configured_archive_prefix().unwrap_or_else(|| PUBNET_PREFIX.to_string())
}

/// Refuse a data-lake folder that belongs to another network than
/// `passphrase`. The network is the folder's `pubnet` / `testnet` segment.
pub fn check_archive_network(prefix: &str, passphrase: &str) -> Result<(), String> {
    let folder_network = prefix.split('/').find_map(|segment| match segment {
        "pubnet" => Some(MAINNET_PASSPHRASE),
        "testnet" => Some(TESTNET_PASSPHRASE),
        _ => None,
    });
    match folder_network {
        Some(expected) if expected == passphrase => Ok(()),
        Some(expected) => Err(format!(
            "ledger folder `{prefix}` holds the network \"{expected}\", \
             but STELLAR_NETWORK_PASSPHRASE is \"{passphrase}\""
        )),
        None => Err(format!(
            "ledger folder `{prefix}` names no known network (pubnet / testnet)"
        )),
    }
}

/// [`check_archive_network`] on this process's configuration
/// (`PUBLIC_ARCHIVE_PREFIX`, `STELLAR_NETWORK_PASSPHRASE`) — the start-up
/// guard of every reader of the data lake.
pub fn check_configured_archive() -> Result<(), String> {
    let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE")
        .map_err(|_| "STELLAR_NETWORK_PASSPHRASE is not set".to_string())?;
    check_archive_network(&public_archive_prefix(), passphrase.trim())
}

#[cfg(test)]
mod tests;
