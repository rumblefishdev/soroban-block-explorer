//! Token metadata extraction from contract **instance storage**.
//!
//! SEP-41 / OpenZeppelin Soroban tokens store `name` / `symbol` / `decimals`
//! as a single struct in the contract's instance storage under the key
//! `Symbol("METADATA")`:
//!
//! ```text
//! METADATA => Map { decimal: U32, name: String, symbol: String }
//! ```
//!
//! Chain-verified on mainnet 2026-06-17 (10/37 live contracts, incl. WASM
//! tokens liquidFi + Comet and every SAC) — see task 0297 notes. This is a
//! DIFFERENT ledger location from the standalone `Symbol("name")` ContractData
//! entry the older name-write path looked for (`state.rs::is_symbol_name_key`),
//! which real tokens do not write — hence `soroban_contracts.name` was a false
//! zero. The instance value reaches this module via `cd.val` (an
//! `ScVal::ContractInstance`) in `ledger_entry_changes.rs`; `scval_to_typed_json`
//! drops `inst.storage`, so the struct must be pulled from the raw `ScVal`.

use stellar_xdr::{ContractExecutable, ScMapEntry, ScVal};

/// Typed token metadata recovered from the instance-storage `METADATA` struct.
///
/// Every field is optional: a conforming token sets all three, but a
/// non-standard or partial struct may omit some, and we extract what is present
/// rather than reject the whole struct.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: Option<u32>,
}

/// Extract the `METADATA` struct from a contract instance value.
///
/// Returns `None` when `val` is not an `ScVal::ContractInstance`, the instance
/// has no storage, there is no `Symbol("METADATA")` key, or the struct yields
/// no usable field.
pub fn extract_token_metadata(val: &ScVal) -> Option<TokenMetadata> {
    let ScVal::ContractInstance(inst) = val else {
        return None;
    };
    let storage = inst.storage.as_ref()?;
    let metadata = storage
        .iter()
        .find(|e| symbol_is(&e.key, "METADATA"))
        .map(|e| &e.val)?;
    let ScVal::Map(Some(fields)) = metadata else {
        return None;
    };

    let mut md = TokenMetadata::default();
    for ScMapEntry { key, val } in fields.iter() {
        match symbol_text(key).as_deref() {
            Some("name") => md.name = scval_text(val),
            Some("symbol") => md.symbol = scval_text(val),
            // SEP-41/OZ tokens use the singular `decimal`; accept `decimals` too.
            Some("decimal") | Some("decimals") => md.decimals = scval_u32(val),
            _ => {}
        }
    }

    if md == TokenMetadata::default() {
        None
    } else {
        Some(md)
    }
}

/// True when `val` is a contract instance whose executable is the native
/// Stellar Asset Contract (SAC). A SAC's name (`CODE:ISSUER`), symbol (= asset
/// code) and decimals (= 7) are derivable from the asset identity, so the
/// metadata side table skips them. This is the **typed** SAC signal (read off
/// the XDR), used by the producer instead of re-deriving it from the serialized
/// `data` JSON — robust even if `data` population is ever trimmed.
pub fn is_stellar_asset_instance(val: &ScVal) -> bool {
    matches!(
        val,
        ScVal::ContractInstance(inst) if inst.executable == ContractExecutable::StellarAsset
    )
}

/// True when `val` is a contract instance whose storage carries a
/// `Symbol("METADATA")` key — regardless of whether the struct under it decodes
/// to anything usable. Lets callers distinguish "not a token / no metadata" from
/// "a token whose METADATA we could not parse" (a non-standard shape worth a log
/// rather than a silent drop).
pub fn has_metadata_key(val: &ScVal) -> bool {
    let ScVal::ContractInstance(inst) = val else {
        return false;
    };
    inst.storage
        .as_ref()
        .is_some_and(|storage| storage.iter().any(|e| symbol_is(&e.key, "METADATA")))
}

/// True when `v` is exactly `Symbol(want)`.
fn symbol_is(v: &ScVal, want: &str) -> bool {
    matches!(v, ScVal::Symbol(s) if s.0.as_slice() == want.as_bytes())
}

/// `Symbol` → its UTF-8 text (used for struct keys).
fn symbol_text(v: &ScVal) -> Option<String> {
    match v {
        ScVal::Symbol(s) => std::str::from_utf8(s.0.as_slice()).ok().map(String::from),
        _ => None,
    }
}

/// Decode a string-y `ScVal` (`String`, `Symbol`, or UTF-8 `Bytes`) to text.
fn scval_text(v: &ScVal) -> Option<String> {
    let bytes = match v {
        ScVal::String(s) => s.0.as_slice(),
        ScVal::Symbol(s) => s.0.as_slice(),
        ScVal::Bytes(b) => b.0.as_slice(),
        _ => return None,
    };
    std::str::from_utf8(bytes).ok().map(String::from)
}

/// Decode a `decimals` value (`U32`, or a small `U64`) to `u32`.
fn scval_u32(v: &ScVal) -> Option<u32> {
    match v {
        ScVal::U32(n) => Some(*n),
        ScVal::U64(n) => u32::try_from(*n).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{
        ContractExecutable, Hash, ScBytes, ScContractInstance, ScMap, ScMapEntry, ScString,
        ScSymbol, ScVal,
    };

    fn sbytes(s: &str) -> ScVal {
        ScVal::Bytes(ScBytes::try_from(s.as_bytes().to_vec()).unwrap())
    }

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol::try_from(s.as_bytes().to_vec()).unwrap())
    }
    fn sstr(s: &str) -> ScVal {
        ScVal::String(ScString::try_from(s.as_bytes().to_vec()).unwrap())
    }
    fn scmap(entries: Vec<(ScVal, ScVal)>) -> ScMap {
        ScMap::try_from(
            entries
                .into_iter()
                .map(|(key, val)| ScMapEntry { key, val })
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn metadata_struct(name: &str, symbol: &str, decimal: u32) -> ScVal {
        // METADATA struct: keys sorted (decimal < name < symbol).
        ScVal::Map(Some(scmap(vec![
            (sym("decimal"), ScVal::U32(decimal)),
            (sym("name"), sstr(name)),
            (sym("symbol"), sstr(symbol)),
        ])))
    }
    fn wasm_instance(storage: Vec<(ScVal, ScVal)>) -> ScVal {
        ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::Wasm(Hash([0xAA; 32])),
            storage: Some(scmap(storage)),
        })
    }

    #[test]
    fn extracts_name_symbol_decimals_from_metadata_struct() {
        // Mirrors mainnet liquidFi bridge token (CDKRSOVB…).
        let inst = wasm_instance(vec![
            (sym("Admin"), ScVal::Void),
            (
                sym("METADATA"),
                metadata_struct("liquidFi bridge token", "lUSDC", 7),
            ),
        ]);
        let m = extract_token_metadata(&inst).expect("METADATA struct present");
        assert_eq!(m.name.as_deref(), Some("liquidFi bridge token"));
        assert_eq!(m.symbol.as_deref(), Some("lUSDC"));
        assert_eq!(m.decimals, Some(7));
    }

    #[test]
    fn returns_none_for_non_instance() {
        assert_eq!(extract_token_metadata(&ScVal::U32(7)), None);
        assert_eq!(extract_token_metadata(&sstr("not an instance")), None);
    }

    #[test]
    fn returns_none_for_instance_without_metadata() {
        // Non-token contract: instance storage present, no METADATA key
        // (e.g. a Blend pool with Admin/Config keys).
        let other = wasm_instance(vec![
            (sym("Admin"), ScVal::Void),
            (sym("Config"), ScVal::U32(1)),
        ]);
        assert_eq!(extract_token_metadata(&other), None);

        // Bachini-style NFT: empty instance storage (chain-verified storageKeys: []).
        let empty = ScVal::ContractInstance(ScContractInstance {
            executable: ContractExecutable::Wasm(Hash([0xAA; 32])),
            storage: Some(scmap(vec![])),
        });
        assert_eq!(extract_token_metadata(&empty), None);
    }

    #[test]
    fn has_metadata_key_distinguishes_present_but_undecodable() {
        // METADATA key present but its value is NOT a Map → `extract` yields
        // None, yet the key IS there. This is the "non-standard token, surface
        // it" case the producer's warn keys off.
        let weird = wasm_instance(vec![(sym("METADATA"), ScVal::U32(42))]);
        assert_eq!(extract_token_metadata(&weird), None);
        assert!(
            has_metadata_key(&weird),
            "METADATA key present though undecodable"
        );

        // No METADATA key → not a token, no signal.
        let other = wasm_instance(vec![(sym("Admin"), ScVal::Void)]);
        assert!(!has_metadata_key(&other));

        // Non-instance → false.
        assert!(!has_metadata_key(&ScVal::U32(7)));
    }

    #[test]
    fn decodes_symbol_and_bytes_typed_fields() {
        // Legal non-standard shapes: name as Bytes(UTF-8), symbol as Symbol.
        let inst = wasm_instance(vec![(
            sym("METADATA"),
            ScVal::Map(Some(scmap(vec![
                (sym("decimal"), ScVal::U32(6)),
                (sym("name"), sbytes("Wrapped BTC")),
                (sym("symbol"), sym("wBTC")),
            ]))),
        )]);
        let m = extract_token_metadata(&inst).expect("METADATA present");
        assert_eq!(m.name.as_deref(), Some("Wrapped BTC"));
        assert_eq!(m.symbol.as_deref(), Some("wBTC"));
        assert_eq!(m.decimals, Some(6));
    }
}
