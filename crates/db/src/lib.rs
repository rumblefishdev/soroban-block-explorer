//! Database access layer for the Soroban block explorer.
//!
//! Provides sqlx PgPool configuration and query functions.
//! Migrations are embedded via `sqlx::migrate!()`.

pub mod pool;
