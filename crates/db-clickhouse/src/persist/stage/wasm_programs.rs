//! WASM programs seen in this ledger: one `wasm_programs` row per hash (the
//! bytes and the metadata read from them), and the per-hash contract-type
//! verdict the deploys use.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{HashMap, HashSet};

use domain::ContractType;
use xdr_parser::types::ExtractedWasmProgram;

use super::{StagedLedger, decode_hash, staging_err};
use crate::SchemaError;
use crate::persist::rows::WasmProgramRow;

pub(super) fn wasm_rows(
    out: &mut StagedLedger,
    programs: &[ExtractedWasmProgram],
) -> Result<HashMap<[u8; 32], ContractType>, SchemaError> {
    // ---- wasm_programs (deduped by wasm_hash) ----
    //
    // Task 0118 Phase 2 (PG-side mirror) — run the wasm-spec classifier
    // alongside the metadata dedup. The resulting per-hash verdict
    // feeds two downstream consumers in this same `prepare` call:
    //   * `contract_rows.contract_type` override for non-SAC deploys
    //     whose WASM is uploaded in the same ledger (matches PG
    //     `Staged::prepare` behaviour at staging.rs:578-585).
    //   * NFT-candidate routing (`nft_rows` / `nft_pending_rows`,
    //     task 0217 / 0220) — `Other`/NULL verdict routes to
    //     quarantine; `Fungible` / `Token` drops the row entirely.
    // A program without an interface section gives no verdict.
    let mut wasm_seen: HashSet<[u8; 32]> = HashSet::new();
    let mut wasm_classification: HashMap<[u8; 32], ContractType> =
        HashMap::with_capacity(programs.len());
    for program in programs {
        let row = program_row(program)?;
        if !wasm_seen.insert(row.wasm_hash) {
            continue;
        }
        if let Some(functions) = &program.functions {
            let classification = xdr_parser::classify_contract_from_wasm_spec(functions);
            wasm_classification.insert(row.wasm_hash, classification.into());
        }
        out.wasm_rows.push(row);
    }

    Ok(wasm_classification)
}

/// The `wasm_programs` row for one program: its bytes and the metadata read
/// from them. Shared with `backfill-runner wasm-code-backfill`, so a program
/// filled from RPC is written exactly like one the indexer saw uploaded.
pub fn program_row(program: &ExtractedWasmProgram) -> Result<WasmProgramRow, SchemaError> {
    // Task 0327: persist the mutability bit so the API can surface the
    // Upgradeable/Immutable badge. Read back via
    // `JSONExtractBool(metadata,'upgradeable')`; rows written before this
    // (no key) read as Unknown → chip renders nothing. A program without an
    // interface section gets empty metadata, which readers treat like no row.
    let metadata = match &program.functions {
        Some(functions) => serde_json::to_string(&serde_json::json!({
            "functions": functions,
            "wasm_byte_len": program.wasm_byte_len,
            "upgradeable": program.upgradeable,
        }))
        .map_err(|e| staging_err(&format!("wasm metadata serialize: {e}")))?,
        None => String::new(),
    };
    Ok(WasmProgramRow {
        wasm_hash: decode_hash(&program.wasm_hash, "wasm_hash")?,
        metadata,
        code: program.code.clone(),
    })
}

#[cfg(test)]
mod tests;
