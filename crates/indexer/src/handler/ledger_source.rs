//! Where the indexer reads ledger files from (task 0553).
//!
//! Mainnet reads the bucket our own Galexie writes, at its root, and S3 rings
//! the indexer for each new file. Testnet reads SDF's public data lake, in its
//! network's folder, and paces itself (`lake_pacing`). `BUCKET_NAME` and
//! `PUBLIC_ARCHIVE_PREFIX` are read once, here, into one of the two.

use xdr_parser::public_archive::{
    PUBLIC_BUCKET, PUBNET_PREFIX, check_archive_network, configured_archive_prefix,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerSource {
    /// The bucket our own Galexie writes, mainnet only; ledger files sit at
    /// its root.
    OwnBucket { bucket: String },
    /// SDF's public data lake; `key_prefix` is the network's folder followed
    /// by `/`, prepended to every ledger file's key.
    PublicLake { key_prefix: String },
}

impl LedgerSource {
    /// Reads `BUCKET_NAME`, `PUBLIC_ARCHIVE_PREFIX` and
    /// `STELLAR_NETWORK_PASSPHRASE`.
    pub fn from_env() -> Result<Self, String> {
        let bucket = std::env::var("BUCKET_NAME").unwrap_or_default();
        let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE")
            .map_err(|_| "STELLAR_NETWORK_PASSPHRASE is not set".to_string())?;
        Self::new(&bucket, configured_archive_prefix(), passphrase.trim())
    }

    /// The network must match the ledgers read: ledger meta carries no
    /// network, so a mismatch would hash every transaction wrong without an
    /// error. A lake folder names its network; our own bucket is mainnet's.
    /// A folder next to our own bucket is refused, because it would be
    /// ignored without a word.
    pub fn new(
        bucket: &str,
        configured_folder: Option<String>,
        passphrase: &str,
    ) -> Result<Self, String> {
        if bucket.is_empty() {
            return Err("BUCKET_NAME env var is missing or empty".to_string());
        }
        if bucket == PUBLIC_BUCKET {
            let folder = configured_folder.unwrap_or_else(|| PUBNET_PREFIX.to_string());
            check_archive_network(&folder, passphrase)?;
            return Ok(LedgerSource::PublicLake {
                key_prefix: format!("{folder}/"),
            });
        }
        if let Some(folder) = configured_folder {
            return Err(format!(
                "PUBLIC_ARCHIVE_PREFIX is `{folder}`, but BUCKET_NAME is `{bucket}`: a ledger \
                 folder is read only from `{PUBLIC_BUCKET}`, so it would be ignored"
            ));
        }
        check_archive_network(PUBNET_PREFIX, passphrase)?;
        Ok(LedgerSource::OwnBucket {
            bucket: bucket.to_string(),
        })
    }

    pub fn bucket(&self) -> &str {
        match self {
            LedgerSource::OwnBucket { bucket } => bucket,
            LedgerSource::PublicLake { .. } => PUBLIC_BUCKET,
        }
    }

    /// Prepended to every ledger file's key: empty at our own bucket's root,
    /// `<folder>/` in the lake.
    pub fn key_prefix(&self) -> &str {
        match self {
            LedgerSource::OwnBucket { .. } => "",
            LedgerSource::PublicLake { key_prefix } => key_prefix,
        }
    }
}

#[cfg(test)]
mod tests;
