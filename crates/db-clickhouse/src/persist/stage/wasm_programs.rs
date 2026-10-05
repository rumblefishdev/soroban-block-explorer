//! WASM programs uploaded in this ledger: one `wasm_interface_metadata` row
//! per hash and the per-hash contract-type verdict the deploys use.
//!
//! Lives in its own file because `stage.rs` is past the module size limit.

use std::collections::{HashMap, HashSet};

use domain::ContractType;
use xdr_parser::types::ExtractedContractInterface;

use super::{StagedLedger, decode_hash, staging_err};
use crate::SchemaError;
use crate::persist::rows::WasmInterfaceMetadataRow;

pub(super) fn wasm_rows(
    out: &mut StagedLedger,
    contract_interfaces: &[ExtractedContractInterface],
) -> Result<HashMap<[u8; 32], ContractType>, SchemaError> {
    // ---- wasm_interface_metadata (deduped by wasm_hash) ----
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
    let mut wasm_seen: HashSet<[u8; 32]> = HashSet::new();
    let mut wasm_classification: HashMap<[u8; 32], ContractType> =
        HashMap::with_capacity(contract_interfaces.len());
    for iface in contract_interfaces {
        let hash = decode_hash(&iface.wasm_hash, "wasm_hash")?;
        if !wasm_seen.insert(hash) {
            continue;
        }
        let classification = xdr_parser::classify_contract_from_wasm_spec(&iface.functions);
        wasm_classification.insert(hash, classification.into());

        // Task 0327: persist the mutability bit so the API can surface the
        // Upgradeable/Immutable badge. Read back via
        // `JSONExtractBool(metadata,'upgradeable')`; rows written before this
        // (no key) read as Unknown → chip renders nothing.
        let metadata = serde_json::json!({
            "functions": iface.functions,
            "wasm_byte_len": iface.wasm_byte_len,
            "upgradeable": iface.upgradeable,
        });
        out.wasm_rows.push(WasmInterfaceMetadataRow {
            wasm_hash: hash,
            metadata: serde_json::to_string(&metadata)
                .map_err(|e| staging_err(&format!("wasm metadata serialize: {e}")))?,
        });
    }

    Ok(wasm_classification)
}
