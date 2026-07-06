//! Database access layer for the Soroban block explorer.
//!
//! Provides sqlx PgPool configuration and query functions.

pub mod pool;

#[cfg(feature = "aws-secrets")]
pub mod secrets;
