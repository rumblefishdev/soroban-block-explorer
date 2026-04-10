-- Staging table for WASM interface metadata (function signatures).
--
-- Task 0104: Soroban's 2-ledger deploy pattern uploads WASM (ContractCodeEntry)
-- in one ledger and deploys the contract (ContractDataEntry) in a later ledger.
-- ExtractedContractInterface is only produced from ContractCodeEntry, so by the
-- time the contract row exists in soroban_contracts, the interface metadata has
-- already been processed and dropped.
--
-- This table persists interface metadata keyed by wasm_hash so it can be applied
-- retroactively when the contract deployment is upserted in a later ledger.
-- Rows here are permanent (wasm bytecode is immutable on-chain); they serve as
-- an index for any future contract deployments using the same WASM.
CREATE TABLE wasm_interface_metadata (
    wasm_hash   VARCHAR(64) PRIMARY KEY,
    metadata    JSONB       NOT NULL
);
