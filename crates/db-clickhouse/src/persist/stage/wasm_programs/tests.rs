//! The program rows staged for one ledger.

use xdr_parser::types::{ContractFunction, ExtractedWasmProgram};

use super::wasm_rows;
use crate::persist::stage::StagedLedger;

fn program(
    hash_byte: u8,
    code: &[u8],
    functions: Option<Vec<ContractFunction>>,
) -> ExtractedWasmProgram {
    ExtractedWasmProgram {
        wasm_hash: hex::encode([hash_byte; 32]),
        functions,
        wasm_byte_len: code.len(),
        upgradeable: false,
        code: code.to_vec(),
    }
}

fn decimals_fn() -> ContractFunction {
    ContractFunction {
        name: "decimals".into(),
        doc: String::new(),
        inputs: Vec::new(),
        outputs: vec!["u32".into()],
    }
}

#[test]
fn each_program_is_one_row_with_its_bytes() {
    // The same program seen twice in one ledger, and a second program.
    let seen = [
        program(1, b"\0asm-one", Some(vec![decimals_fn()])),
        program(1, b"\0asm-one", Some(vec![decimals_fn()])),
        program(2, b"\0asm-two", Some(Vec::new())),
    ];
    let mut out = StagedLedger::default();
    let verdicts = wasm_rows(&mut out, &seen).unwrap();

    let stored: Vec<_> = out
        .wasm_rows
        .iter()
        .map(|r| (r.wasm_hash[0], r.code.as_slice()))
        .collect();
    assert_eq!(
        stored,
        vec![(1, b"\0asm-one".as_slice()), (2, b"\0asm-two".as_slice())]
    );
    assert!(out.wasm_rows[0].metadata.contains("\"decimals\""));
    assert_eq!(verdicts.len(), 2);
}

#[test]
fn a_program_without_an_interface_keeps_its_bytes_and_gives_no_verdict() {
    let mut out = StagedLedger::default();
    let verdicts = wasm_rows(&mut out, &[program(3, b"\0asm-bare", None)]).unwrap();

    assert_eq!(out.wasm_rows.len(), 1);
    assert_eq!(out.wasm_rows[0].code, b"\0asm-bare");
    // Empty metadata reads like "no interface", as a missing row did before.
    assert_eq!(out.wasm_rows[0].metadata, "");
    assert!(verdicts.is_empty());
}
