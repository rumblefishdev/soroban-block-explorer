//! Indexer library — exposes the processing pipeline for reuse by other crates
//! (e.g. backfill-runner). The Lambda entry point remains in main.rs.

pub mod contract_metadata;
pub mod contract_state;
pub mod handler;
