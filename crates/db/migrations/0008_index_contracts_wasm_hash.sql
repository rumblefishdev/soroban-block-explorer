-- Index on wasm_hash to support contract interface metadata join.
-- Task 0104: ExtractedContractInterface carries wasm_hash, not contract_id.
-- This index enables efficient lookup of all contracts sharing a given WASM bytecode.
CREATE INDEX idx_contracts_wasm_hash ON soroban_contracts (wasm_hash)
    WHERE wasm_hash IS NOT NULL;
