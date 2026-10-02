//! Where the indexer reads ledger files from (task 0553).
//!
//! Mainnet reads the bucket our own Galexie writes, at its root, and S3 rings
//! the indexer for each new file. Testnet reads SDF's public data lake, in its
//! network's folder, and paces itself (`lake_pacing`). `BUCKET_NAME` and
//! `PUBLIC_ARCHIVE_PREFIX` are read once, here, into one of the two.

use xdr_parser::public_archive::{
    FIRST_CLOSED_LEDGER, PUBLIC_BUCKET, PUBNET_PREFIX, check_archive_network,
    configured_archive_prefix,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerSource {
    /// The bucket our own Galexie writes; ledger files sit at its root.
    OwnBucket { bucket: String },
    /// SDF's public data lake; `prefix` is the network's folder, no trailing
    /// slash.
    PublicLake { prefix: String },
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

    /// A lake folder must belong to the configured network: ledger meta
    /// carries no network, so a mismatch would hash every transaction wrong
    /// without an error. A folder next to our own bucket is refused, because
    /// it would be ignored without a word.
    pub fn new(
        bucket: &str,
        configured_prefix: Option<String>,
        passphrase: &str,
    ) -> Result<Self, String> {
        if bucket.is_empty() {
            return Err("BUCKET_NAME env var is missing or empty".to_string());
        }
        if bucket == PUBLIC_BUCKET {
            let prefix = configured_prefix.unwrap_or_else(|| PUBNET_PREFIX.to_string());
            check_archive_network(&prefix, passphrase)?;
            return Ok(LedgerSource::PublicLake { prefix });
        }
        match configured_prefix {
            Some(prefix) => Err(format!(
                "PUBLIC_ARCHIVE_PREFIX is `{prefix}`, but BUCKET_NAME is `{bucket}`: a ledger \
                 folder is read only from `{PUBLIC_BUCKET}`, so it would be ignored"
            )),
            None => Ok(LedgerSource::OwnBucket {
                bucket: bucket.to_string(),
            }),
        }
    }

    pub fn bucket(&self) -> &str {
        match self {
            LedgerSource::OwnBucket { bucket } => bucket,
            LedgerSource::PublicLake { .. } => PUBLIC_BUCKET,
        }
    }

    /// Prepended to every ledger file's key: empty at our own bucket's root,
    /// `<folder>/` in the lake.
    pub fn key_prefix(&self) -> String {
        match self {
            LedgerSource::OwnBucket { .. } => String::new(),
            LedgerSource::PublicLake { prefix } => format!("{prefix}/"),
        }
    }

    /// The ledger an empty database starts from. The lake holds every ledger
    /// from the network's first closed one, so testnet rebuilds itself after a
    /// reset. Our own bucket holds ledgers only from where our Galexie started,
    /// so an empty mainnet database is seeded by a backfill first: none.
    pub fn first_ledger(&self) -> Option<i64> {
        match self {
            LedgerSource::OwnBucket { .. } => None,
            LedgerSource::PublicLake { .. } => Some(i64::from(FIRST_CLOSED_LEDGER)),
        }
    }
}

#[cfg(test)]
mod tests;
