use super::*;

#[test]
fn upgrade_import_present_is_upgradeable() {
    // ("l","6") = update_current_contract_wasm
    assert!(wasm_imports_upgrade_fn(&wasm_with_imports(&[(b"l", b"6")])));
}

#[test]
fn no_upgrade_import_is_frozen() {
    // ("l","7") = extend_contract_data_ttl — not the upgrade fn
    assert!(!wasm_imports_upgrade_fn(&wasm_with_imports(&[(
        b"l", b"7"
    )])));
}

/// Unsigned LEB128 encoder — used so the test builder emits correct 2-byte
/// lengths for names / sections ≥ 128 bytes (exercises the multi-byte
/// `read_leb128` path, not just the single-byte happy case).
fn leb(mut v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
    out
}

/// Minimal WASM: magic + version + an import section of `(module, field)`
/// func imports. LEB128-correct for any name / section length, so it covers
/// single-, multi-import (descriptor-skip), and ≥128-byte-name cases.
fn wasm_with_imports(imports: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut body = leb(imports.len() as u32); // import count
    for (module, field) in imports {
        body.extend_from_slice(&leb(module.len() as u32));
        body.extend_from_slice(module);
        body.extend_from_slice(&leb(field.len() as u32));
        body.extend_from_slice(field);
        body.extend_from_slice(&[0x00, 0x00]); // kind=func(0), typeidx=0
    }
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\x00asm");
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    wasm.push(0x02); // import section id
    wasm.extend_from_slice(&leb(body.len() as u32)); // section size
    wasm.extend_from_slice(&body);
    wasm
}

#[test]
fn upgrade_import_not_first_is_found() {
    let wasm = wasm_with_imports(&[(b"l", b"_"), (b"x", b"0"), (b"l", b"6")]);
    assert!(wasm_imports_upgrade_fn(&wasm));
}

#[test]
fn many_imports_without_upgrade_is_frozen() {
    let wasm = wasm_with_imports(&[(b"l", b"_"), (b"l", b"7"), (b"d", b"0")]);
    assert!(!wasm_imports_upgrade_fn(&wasm));
}

#[test]
fn non_func_import_named_l6_is_not_upgrade() {
    // A GLOBAL import literally named ("l","6") is NOT the upgrade host fn
    // (which is a func import) — must not be flagged self-upgradeable.
    let mut body = vec![0x01]; // 1 import
    body.extend_from_slice(&[0x01, b'l']); // module "l"
    body.extend_from_slice(&[0x01, b'6']); // field "6"
    body.extend_from_slice(&[0x03, 0x7f, 0x00]); // kind=global, valtype i32, immutable
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\x00asm");
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    wasm.push(0x02);
    wasm.push(body.len() as u8);
    wasm.extend_from_slice(&body);
    assert!(!wasm_imports_upgrade_fn(&wasm));
}

#[test]
fn multibyte_import_name_is_walked_and_upgrade_found() {
    // A ≥128-byte module name forces a 2-byte LEB length; the walker must
    // consume it correctly and still find the ("l","6") import after it.
    let long_name = vec![b'a'; 200];
    let wasm = wasm_with_imports(&[(&long_name, b"fn"), (b"l", b"6")]);
    assert!(wasm_imports_upgrade_fn(&wasm));
    // …and without the upgrade import it stays frozen (length-skip is exact).
    let wasm = wasm_with_imports(&[(&long_name, b"fn"), (b"l", b"7")]);
    assert!(!wasm_imports_upgrade_fn(&wasm));
}

#[test]
fn non_wasm_is_not_upgradeable() {
    assert!(!wasm_imports_upgrade_fn(b"not a wasm"));
    assert!(!wasm_imports_upgrade_fn(&[]));
}

#[test]
fn read_leb128_single_byte() {
    assert_eq!(read_leb128(&[0x05]), Some((5, 1)));
    assert_eq!(read_leb128(&[0x7F]), Some((127, 1)));
}

#[test]
fn read_leb128_multi_byte() {
    // 128 = 0x80 0x01
    assert_eq!(read_leb128(&[0x80, 0x01]), Some((128, 2)));
    // 300 = 0xAC 0x02
    assert_eq!(read_leb128(&[0xAC, 0x02]), Some((300, 2)));
}

#[test]
fn extract_custom_section_from_minimal_wasm() {
    // Build a minimal WASM with a custom section named "test" containing [1,2,3]
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\x00asm"); // magic
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // version 1

    // Custom section: id=0, size=8, name_len=4, name="test", content=[1,2,3]
    wasm.push(0x00); // section id = custom
    wasm.push(0x08); // section size = 8 bytes
    wasm.push(0x04); // name length = 4
    wasm.extend_from_slice(b"test"); // name
    wasm.extend_from_slice(&[1, 2, 3]); // content

    let result = extract_custom_section(&wasm, "test");
    assert_eq!(result, Some(vec![1, 2, 3]));
}

#[test]
fn extract_custom_section_not_found() {
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\x00asm");
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    // Custom section with name "other"
    wasm.push(0x00);
    wasm.push(0x09);
    wasm.push(0x05);
    wasm.extend_from_slice(b"other");
    wasm.extend_from_slice(&[1, 2, 3]);

    let result = extract_custom_section(&wasm, "contractspecv0");
    assert!(result.is_none());
}

#[test]
fn extract_custom_section_skips_non_custom() {
    let mut wasm = Vec::new();
    wasm.extend_from_slice(b"\x00asm");
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    // Non-custom section (type section, id=1)
    wasm.push(0x01); // section id = type
    wasm.push(0x03); // section size = 3
    wasm.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // section data

    // Custom section with target name
    wasm.push(0x00); // custom
    wasm.push(0x07); // size = 7
    wasm.push(0x04); // name len = 4
    wasm.extend_from_slice(b"test");
    wasm.extend_from_slice(&[42, 43]);

    let result = extract_custom_section(&wasm, "test");
    assert_eq!(result, Some(vec![42, 43]));
}

#[test]
fn invalid_wasm_returns_none() {
    assert!(extract_custom_section(&[], "test").is_none());
    assert!(extract_custom_section(b"not wasm", "test").is_none());
}

#[test]
fn no_interfaces_for_non_soroban_meta() {
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let result = extract_wasm_programs(&tx_meta);
    assert!(result.is_empty());
}

#[test]
fn spec_type_to_string_primitives() {
    assert_eq!(spec_type_to_string(&ScSpecTypeDef::Bool), "bool");
    assert_eq!(spec_type_to_string(&ScSpecTypeDef::U128), "u128");
    assert_eq!(spec_type_to_string(&ScSpecTypeDef::Address), "address");
}

#[test]
fn spec_type_to_string_compound() {
    let opt = ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
        value_type: Box::new(ScSpecTypeDef::Address),
    }));
    assert_eq!(spec_type_to_string(&opt), "option<address>");

    let vec = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
        element_type: Box::new(ScSpecTypeDef::U64),
    }));
    assert_eq!(spec_type_to_string(&vec), "vec<u64>");

    let map = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
        key_type: Box::new(ScSpecTypeDef::Symbol),
        value_type: Box::new(ScSpecTypeDef::I128),
    }));
    assert_eq!(spec_type_to_string(&map), "map<symbol, i128>");
}
