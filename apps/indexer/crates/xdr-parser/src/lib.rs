//! XDR parser for the Soroban block explorer indexer.
//!
//! Deserializes `LedgerCloseMeta` payloads from Galexie S3 exports and extracts
//! structured ledger and transaction data for PostgreSQL persistence.
//!
//! This is the sole XDR parsing path (ADR-0004: Rust-only, ingestion-time).

pub mod error;
pub mod ledger;
pub mod memo;
pub mod transaction;
pub mod types;

mod envelope;
mod xdr_limits;

pub use error::{ParseError, ParseErrorKind};
pub use ledger::extract_ledger;
pub use transaction::extract_transactions;
pub use types::{ExtractedLedger, ExtractedTransaction};

use stellar_xdr::curr::{LedgerCloseMetaBatch, ReadXdr};

/// Decompress a zstd-compressed XDR payload.
pub fn decompress_zstd(compressed: &[u8]) -> Result<Vec<u8>, ParseError> {
    zstd::decode_all(compressed).map_err(|e| ParseError {
        kind: ParseErrorKind::DecompressionFailed,
        message: e.to_string(),
        context: None,
    })
}

/// Deserialize a `LedgerCloseMetaBatch` from raw XDR bytes.
pub fn deserialize_batch(xdr_bytes: &[u8]) -> Result<LedgerCloseMetaBatch, ParseError> {
    let limits = xdr_limits::deserialization_limits(xdr_bytes.len());
    LedgerCloseMetaBatch::from_xdr(xdr_bytes, limits).map_err(|e| ParseError {
        kind: ParseErrorKind::XdrDeserializationFailed,
        message: e.to_string(),
        context: None,
    })
}

/// Parse an S3 object key to extract the ledger sequence range.
///
/// Expected format: `stellar-ledger-data/ledgers/{seq_start}-{seq_end}.xdr.zstd`
pub fn parse_s3_key(key: &str) -> Result<(u32, u32), ParseError> {
    let filename = key
        .rsplit('/')
        .next()
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::InvalidS3Key,
            message: format!("no filename in key: {key}"),
            context: None,
        })?
        .strip_suffix(".xdr.zstd")
        .ok_or_else(|| ParseError {
            kind: ParseErrorKind::InvalidS3Key,
            message: format!("key does not end with .xdr.zstd: {key}"),
            context: None,
        })?;

    let (start_str, end_str) = filename.split_once('-').ok_or_else(|| ParseError {
        kind: ParseErrorKind::InvalidS3Key,
        message: format!("filename missing dash separator: {filename}"),
        context: None,
    })?;

    let start = start_str.parse::<u32>().map_err(|e| ParseError {
        kind: ParseErrorKind::InvalidS3Key,
        message: format!("invalid start sequence: {e}"),
        context: None,
    })?;
    let end = end_str.parse::<u32>().map_err(|e| ParseError {
        kind: ParseErrorKind::InvalidS3Key,
        message: format!("invalid end sequence: {e}"),
        context: None,
    })?;

    if start > end {
        return Err(ParseError {
            kind: ParseErrorKind::InvalidS3Key,
            message: format!("start sequence {start} > end sequence {end}"),
            context: None,
        });
    }

    Ok((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_s3_key_valid() {
        let (start, end) =
            parse_s3_key("stellar-ledger-data/ledgers/61827430-61827440.xdr.zstd").unwrap();
        assert_eq!(start, 61827430);
        assert_eq!(end, 61827440);
    }

    #[test]
    fn parse_s3_key_invalid_suffix() {
        assert!(parse_s3_key("ledgers/100-200.xdr").is_err());
    }

    #[test]
    fn parse_s3_key_missing_dash() {
        assert!(parse_s3_key("ledgers/100200.xdr.zstd").is_err());
    }

    #[test]
    fn parse_s3_key_start_greater_than_end() {
        assert!(parse_s3_key("ledgers/200-100.xdr.zstd").is_err());
    }
}
