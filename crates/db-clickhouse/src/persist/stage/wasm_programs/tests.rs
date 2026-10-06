//! The program rows staged for one ledger.

use xdr_parser::types::ExtractedContractInterface;

use super::wasm_rows;
use crate::persist::stage::StagedLedger;

fn program(hash_byte: u8, code: &[u8]) -> ExtractedContractInterface {
    ExtractedContractInterface {
        wasm_hash: hex::encode([hash_byte; 32]),
        functions: Vec::new(),
        wasm_byte_len: code.len(),
        upgradeable: false,
        code: code.to_vec(),
    }
}

#[test]
fn each_uploaded_program_keeps_its_bytes_once() {
    // The same program uploaded twice in one ledger, and a second program.
    let uploads = [
        program(1, b"\0asm-one"),
        program(1, b"\0asm-one"),
        program(2, b"\0asm-two"),
    ];
    let mut out = StagedLedger::default();
    wasm_rows(&mut out, &uploads).unwrap();

    let stored: Vec<_> = out
        .wasm_code_rows
        .iter()
        .map(|r| (r.wasm_hash[0], r.code.as_slice()))
        .collect();
    assert_eq!(
        stored,
        vec![(1, b"\0asm-one".as_slice()), (2, b"\0asm-two".as_slice())]
    );
    assert_eq!(out.wasm_rows.len(), 2);
}
