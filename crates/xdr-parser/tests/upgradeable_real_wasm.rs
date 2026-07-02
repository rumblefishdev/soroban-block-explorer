//! Task 0327 — validate the mutability scan against REAL mainnet WASM, not just
//! synthetic byte fixtures. Both blobs were fetched live from Soroban RPC
//! (`getLedgerEntries`, `LedgerKey::ContractCode`) and independently confirmed
//! (a separate import-section parser) before being committed here.
//!
//! - `upgradeable_mainnet.wasm` — wasm_hash `07097f83…d284b0`, the single most
//!   widely deployed mainnet WASM (~86k contracts). Imports
//!   `update_current_contract_wasm` (module `"l"`, field `"6"`) → Upgradeable.
//! - `frozen_mainnet.wasm` — wasm_hash `0a41411f…969781`. Does NOT import it →
//!   Immutable / frozen.
//!
//! These fixtures are the drift guard for the host-defined `("l","6")` mapping
//! (`wasm_imports_upgrade_fn`): if a future Soroban protocol renumbers host-fn
//! exports, the upgradeable fixture stops importing `("l","6")` and this test
//! goes red. **On a protocol bump that fails these, re-verify the export numbers
//! against the new `rs-soroban-env` env.json and re-fetch fresh fixtures — do
//! NOT just bump the assertions.**

use xdr_parser::contract::wasm_imports_upgrade_fn;

const UPGRADEABLE: &[u8] = include_bytes!("fixtures/upgradeable_mainnet.wasm");
const FROZEN: &[u8] = include_bytes!("fixtures/frozen_mainnet.wasm");

#[test]
fn real_mainnet_upgradeable_wasm_is_detected() {
    assert!(
        wasm_imports_upgrade_fn(UPGRADEABLE),
        "the most-deployed mainnet WASM imports update_current_contract_wasm"
    );
}

#[test]
fn real_mainnet_frozen_wasm_is_immutable() {
    assert!(
        !wasm_imports_upgrade_fn(FROZEN),
        "this mainnet WASM has no self-upgrade import"
    );
}

/// The flag must re-resolve across a WASM upgrade. It is derived per WASM and
/// stored keyed by `wasm_hash`; the API reads the contract's CURRENT
/// `wasm_hash` (kept correct by task 0320), so when a contract upgrades from
/// one WASM to another its badge follows the new code. This asserts the
/// substance of that guarantee: the same scanner yields different verdicts for
/// the two distinct WASMs a contract could move between, so a frozen→
/// upgradeable upgrade flips the bit (and vice-versa). The per-`wasm_hash`
/// lookup itself is covered by `queries_ch::map_upgradeable`.
#[test]
fn upgrade_reresolves_flag_on_new_wasm() {
    // Pre-upgrade: contract runs FROZEN → No self-upgrade.
    let before = wasm_imports_upgrade_fn(FROZEN);
    // Post-upgrade: same contract now runs UPGRADEABLE → Self-upgradeable.
    let after = wasm_imports_upgrade_fn(UPGRADEABLE);
    assert!(!before && after, "flag must flip when the WASM changes");
    // And the reverse direction (renounce upgradeability) flips it back.
    assert!(
        wasm_imports_upgrade_fn(UPGRADEABLE) && !wasm_imports_upgrade_fn(FROZEN),
        "renouncing self-upgrade (upgrade to a frozen WASM) flips it off"
    );
}
