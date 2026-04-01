//! Ledger domain type matching the `ledgers` PostgreSQL table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Ledger record as stored in PostgreSQL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    /// Ledger sequence number (PK).
    pub sequence: i64,
    /// SHA-256 hash of the ledger header, hex-encoded (64 chars).
    pub hash: String,
    /// Ledger close timestamp.
    pub closed_at: DateTime<Utc>,
    /// Stellar protocol version.
    pub protocol_version: i32,
    /// Number of transactions in this ledger.
    pub transaction_count: i32,
    /// Base fee in stroops.
    pub base_fee: i64,
}
