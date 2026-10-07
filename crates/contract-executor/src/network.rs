//! The network settings a run needs, as constants (task 0620).
//!
//! Read from mainnet's config-setting entries at ledger 64,818,615 (protocol
//! 29) via `getLedgerEntries`. They change only with a network upgrade, which
//! also needs a new `soroban-env-host`, so they are refreshed with that bump.
//! For a read-only call they bound the work and price it; the value returned
//! does not depend on them, so a network with slightly different costs
//! (testnet) gets the same answers.

/// `ContractComputeV0.tx_max_instructions`.
pub const TX_MAX_INSTRUCTIONS: u64 = 400_000_000;
/// `ContractComputeV0.tx_memory_limit` (40 MiB).
pub const TX_MEMORY_LIMIT: u64 = 41_943_040;

/// `StateArchival.min_temporary_ttl`.
pub const MIN_TEMPORARY_TTL: u32 = 17_280;
/// `StateArchival.min_persistent_ttl`.
pub const MIN_PERSISTENT_TTL: u32 = 2_073_600;
/// `StateArchival.max_entry_ttl`.
pub const MAX_ENTRY_TTL: u32 = 3_110_400;

/// Base reserve in stroops; read only by operations that create accounts.
pub const BASE_RESERVE: u32 = 5_000_000;

/// `LedgerEntryData` XDR of the `ContractCostParamsCpuInstructions` setting.
pub const COST_PARAMS_CPU_XDR: &str = include_str!("network/cost_params_cpu.xdr.b64");
/// `LedgerEntryData` XDR of the `ContractCostParamsMemoryBytes` setting.
pub const COST_PARAMS_MEM_XDR: &str = include_str!("network/cost_params_mem.xdr.b64");
