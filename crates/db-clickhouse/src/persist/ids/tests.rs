use super::*;

/// Determinism — same input ⇒ same output. Load-bearing for
/// ReplacingMergeTree replay-idempotency.
#[test]
fn determinism() {
    let g = "GABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUV";
    let c = "CABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUV";
    let h = [7u8; 32];
    assert_eq!(account_id(g), account_id(g));
    assert_eq!(contract_id(c), contract_id(c));
    assert_eq!(transaction_id(&h), transaction_id(&h));
}

/// Different inputs ⇒ different outputs (single-byte sensitivity).
#[test]
fn distinct_inputs_yield_distinct_ids() {
    assert_ne!(account_id("GA"), account_id("GB"));
    assert_ne!(contract_id("CA"), contract_id("CB"));
    assert_ne!(transaction_id(&[0u8; 32]), transaction_id(&[1u8; 32]));
}

/// Cross-table FK consistency — every account `_id` FK across the
/// schema must come from this single helper. Test asserts the
/// invariant by computing twice and comparing.
#[test]
fn fk_consistency_account_id() {
    let key = "GFOOBAR0123";
    // Same value used for `accounts.id` AND every FK column
    // (`source_id`, `caller_id`, etc.).
    assert_eq!(account_id(key), account_id(key));
}

/// GOLDEN — pins the cityhash surrogate of known inputs to their exact `i64`.
/// These bytes are load-bearing: they key `balances.holder_id` / `.asset_id`,
/// `assets.id`, and every account/contract FK across the schema. A change to
/// the hash (crate swap, seed, byte handling) silently RE-KEYS the whole DB
/// and orphans every prior row — this test makes such a change fail LOUDLY.
/// Do NOT "update the expected value" to make it pass: if it breaks, the hash
/// changed and every table needs a full re-backfill (see the module header).
#[test]
fn golden_surrogate_values_are_pinned() {
    let g = "GAWOKP6NJAWNRPQDE4O3NZYDFJHEMLUIP36AC74HNBHLTA3GURYB4PYJ";
    let c = "CCSNFZ5RA2EHTSMK2A5ZDXRCAQBYBVFAPJFNWP5BJECLIL4J5UBLLUQG";
    assert_eq!(account_id(g), 4_204_727_763_610_853_148);
    // `address_id` shares the account/contract surrogate space (task 0331).
    assert_eq!(address_id(g), 4_204_727_763_610_853_148);
    assert_eq!(contract_id(c), -8_283_827_203_770_785_938);
    assert_eq!(asset_id(0, "", 0, 0), -6_959_166_271_784_855_184); // native
    assert_eq!(NATIVE_ASSET_ID, asset_id(0, "", 0, 0)); // const == fn (dedup pin)
    assert_eq!(
        asset_id(1, "USDC", account_id(g), 0),
        -5_142_557_507_226_545_233
    ); // classic "CODE:issuer"
    // type-3 asset_id IS the token's own contract surrogate (identity, no re-hash).
    assert_eq!(asset_id(3, "", 0, contract_id(c)), contract_id(c));
}

/// `asset_id` (task 0331) — surrogate for the unified `balances.asset_id`.
/// native→"native"; classic→"CODE:ISSUER"; soroban (type-3)→the contract
/// surrogate itself. The fn is TOTAL, so the retired type-2 (SAC) input also maps
/// to the contract surrogate — but ADR 0051 re-keys SAC balances onto the classic
/// id, so no type-2 id is ever stored; the type-2 cases below only pin the raw fn.
#[test]
fn asset_id_canonical() {
    let iss = account_id("GISSUER");
    let csac = contract_id("CSAC");
    let ctok = contract_id("CTOKEN1");
    // soroban (3): asset_id == its OWN contract surrogate.
    assert_eq!(asset_id(3, "", 0, ctok), ctok);
    // type-2 (SAC) is RETIRED (ADR 0051): the total fn still maps it to the
    // contract surrogate, but a SAC balance is re-keyed to its classic id, so this
    // value is never persisted — asserted only to document the raw fn.
    assert_eq!(asset_id(2, "USDC", iss, csac), csac);
    // The raw fn gives classic and the (retired) type-2 surrogate DISTINCT values —
    // which is exactly WHY the re-key exists: map that surrogate → the classic id.
    assert_ne!(asset_id(1, "USDC", iss, 0), asset_id(2, "USDC", iss, csac));
    // Distinct classics differ; native is its own thing; deterministic.
    assert_ne!(asset_id(1, "USDC", iss, 0), asset_id(1, "EURC", iss, 0));
    assert_ne!(asset_id(0, "", 0, 0), asset_id(1, "USDC", iss, 0));
    assert_eq!(asset_id(0, "", 0, 0), asset_id(0, "", 0, 0));
}

/// `credit_asset_id` MUST equal the raw 4-arg classic formula — it is the one
/// canonical helper all credit-asset call sites share (task 0393), so this
/// pins the equivalence and catches any drift.
#[test]
fn credit_asset_id_matches_raw_formula() {
    assert_eq!(
        credit_asset_id("USDC", "GISSUER"),
        asset_id(1, "USDC", account_id("GISSUER"), 0)
    );
}
