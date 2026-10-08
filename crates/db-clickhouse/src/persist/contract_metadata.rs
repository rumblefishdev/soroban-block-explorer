//! A contract's `name`, `symbol` and `decimals`, as its own SEP-41 (token) or
//! SEP-50 (NFT) functions return them, and the `soroban_contract_metadata`
//! write that carries them. The indexer reads them by running those functions
//! (`indexer::contract_metadata`, task 0620); the parser does not read them.

/// What a contract's `name`, `symbol` and `decimals` functions returned.
///
/// Every field is optional: a program declares only some of the three (an NFT
/// has no `decimals`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u32>,
}

/// One `soroban_contract_metadata` write for a contract: what its `name`,
/// `symbol` and `decimals` functions returned when its instance changed. Made
/// by the indexer, not the parser (`indexer::contract_metadata`,
/// task 0620). A separate per-contract table, composed at read time — never
/// mixed into `soroban_contracts` (RMT whole-row clobber + different update
/// clocks).
#[derive(Debug, Clone)]
pub struct ExtractedContractMetadata {
    pub contract_id: String,
    pub metadata: TokenMetadata,
    /// Ledger the functions were run at — the side table's RMT version slot.
    pub ledger: u32,
}
