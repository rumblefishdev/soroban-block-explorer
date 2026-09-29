//! Where the public Stellar ledger data lake keeps each network's files, and
//! the guard that a configured folder matches the network being parsed.
//!
//! `s3://aws-public-blockchain/v1.1/stellar/ledgers/` holds `pubnet/` and one
//! `testnet/<genesis-date>/` folder per testnet reset (lore-0553). A folder of
//! the other network parses without error — ledger meta carries no network —
//! so a mismatch would silently hash every transaction wrong. Hence the check.

use crate::{MAINNET_PASSPHRASE, TESTNET_PASSPHRASE};

/// Mainnet's folder inside `aws-public-blockchain`.
pub const PUBNET_PREFIX: &str = "v1.1/stellar/ledgers/pubnet";

/// The configured folder: `PUBLIC_ARCHIVE_PREFIX`, else [`PUBNET_PREFIX`].
/// No trailing slash.
pub fn public_archive_prefix() -> String {
    std::env::var("PUBLIC_ARCHIVE_PREFIX")
        .ok()
        .map(|p| p.trim().trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| PUBNET_PREFIX.to_string())
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

#[cfg(test)]
mod tests;
