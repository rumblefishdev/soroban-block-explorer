//! A contract's `name`, `symbol` and `decimals`, as its own SEP-41 (token) or
//! SEP-50 (NFT) functions return them. The indexer reads them by running those
//! functions (`indexer::token_metadata_by_functions`, task 0620); the parser
//! does not read them.

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
