use super::*;
use serde_json::json;
use std::collections::HashMap;

fn make_change(
    entry_type: &str,
    change_type: &str,
    key: Value,
    data: Option<Value>,
) -> ExtractedLedgerEntryChange {
    ExtractedLedgerEntryChange {
        transaction_hash: "abc123".into(),
        change_type: change_type.into(),
        entry_type: entry_type.into(),
        key,
        data,
        change_index: 0,
        operation_index: None,
        ledger_sequence: 100,
        created_at: 1700000000,
        token_metadata: None,
    }
}

// -- lore-0356: LP snapshot = deterministic end-of-ledger image (keep-last) --

fn lp_change(
    change_type: &str,
    pool_id: &str,
    reserve_a: i64,
    reserve_b: i64,
    shares: i64,
) -> ExtractedLedgerEntryChange {
    make_change(
        "liquidity_pool",
        change_type,
        json!({}),
        Some(json!({
            "pool_id": pool_id,
            "params": { "asset_a": null, "asset_b": null, "fee": 30 },
            "reserve_a": reserve_a,
            "reserve_b": reserve_b,
            "total_pool_shares": shares,
        })),
    )
}

fn lp_removed(pool_id: &str) -> ExtractedLedgerEntryChange {
    make_change(
        "liquidity_pool",
        "removed",
        json!({ "pool_id": pool_id }),
        None,
    )
}

#[test]
fn a_state_image_is_never_a_value() {
    // Core pairs every `updated` with the `state` it replaced (the image at the
    // start of the operation). Only the `updated` carries the pool's value.
    let (pools, snapshots) = extract_liquidity_pools(&[
        lp_change("state", "POOL1", 100, 200, 50),
        lp_change("updated", "POOL1", 110, 182, 50),
    ]);
    assert_eq!(pools.len(), 1);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].reserves, json!({ "a": 110, "b": 182 }));
}

#[test]
fn a_lone_state_writes_nothing() {
    // Cannot happen on chain (core always follows `state` with `updated` or
    // `removed`); if it ever does, it is logged, not stored as the pool's value.
    let (pools, snapshots) = extract_liquidity_pools(&[lp_change("state", "POOL1", 100, 200, 50)]);
    assert!(pools.is_empty());
    assert!(snapshots.is_empty());
}

#[test]
fn dedup_keeps_last_image_per_pool_ledger() {
    // Multi-op pool: keep the last (end-of-ledger) image, not before/intermediate.
    let (_pools, snapshots) = extract_liquidity_pools(&[
        lp_change("state", "POOL1", 100, 200, 50),
        lp_change("updated", "POOL1", 110, 190, 50),
        lp_change("state", "POOL1", 110, 190, 50),
        lp_change("updated", "POOL1", 121, 181, 50), // final
        lp_change("updated", "POOL2", 7, 8, 3),
    ]);
    assert_eq!(snapshots.len(), 3, "a snapshot per updated, none per state");

    let deduped = dedup_final_pool_snapshots(snapshots);
    assert_eq!(deduped.len(), 2, "one snapshot per (pool, ledger)");
    let p1 = deduped.iter().find(|s| s.pool_id == "POOL1").unwrap();
    assert_eq!(
        p1.reserves,
        json!({ "a": 121, "b": 181 }),
        "final image, not before/intermediate"
    );
}

#[test]
fn a_removed_pool_ends_the_ledger_at_zero() {
    // The revocation shape (task 0210): an earlier trade in the ledger, then one
    // operation whose meta is the start-of-operation `state` with full reserves
    // and the `removed`. The pool holds nothing at ledger close.
    let (pools, snapshots) = extract_liquidity_pools(&[
        lp_change("state", "POOL1", 90, 210, 50),
        lp_change("updated", "POOL1", 100, 200, 50),
        lp_change("state", "POOL1", 100, 200, 50),
        lp_removed("POOL1"),
    ]);
    let deduped = dedup_final_pool_snapshots(snapshots);
    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].reserves, json!({ "a": 0, "b": 0 }));
    assert_eq!(deduped[0].total_shares, "0");

    // The pool row still carries the params, from the `state` before the removal.
    let last = pools.last().unwrap();
    assert_eq!(last.fee_bps, 30);
    assert_eq!(last.reserves, json!({ "a": 0, "b": 0 }));
    assert!(last.created_at_ledger.is_none());
}

#[test]
fn a_removal_without_its_state_still_zeroes_the_pool() {
    let (pools, snapshots) = extract_liquidity_pools(&[lp_removed("POOL1")]);
    assert!(pools.is_empty(), "no params to build a pool row from");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].reserves, json!({ "a": 0, "b": 0 }));
}

// -- Contract Deployment Tests --

#[test]
fn extract_wasm_contract_deployment() {
    let changes = vec![make_change(
        "contract_data",
        "created",
        json!({
            "contract": "CABC123",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
        }),
        Some(json!({
            "contract": "CABC123",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
            "val": { "type": "contract_instance", "value": {
                "executable": { "type": "wasm", "hash": "aa".repeat(32) }
            }},
        })),
    )];

    let deployments =
        extract_contract_deployments(&changes, "GDEPLOYER", &HashMap::new(), &HashMap::new());
    assert_eq!(deployments.len(), 1);
    assert_eq!(deployments[0].contract_id, "CABC123");
    assert_eq!(
        deployments[0].deployer_account.as_deref(),
        Some("GDEPLOYER")
    );
    assert_eq!(deployments[0].wasm_hash, Some("aa".repeat(32)));
    assert!(!deployments[0].is_sac);
    assert_eq!(deployments[0].contract_type, ContractType::Other);
}

#[test]
fn extract_sac_deployment() {
    let changes = vec![make_change(
        "contract_data",
        "created",
        json!({
            "contract": "CSAC456",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
        }),
        Some(json!({
            "contract": "CSAC456",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
            "val": { "type": "contract_instance", "value": {
                "executable": { "type": "stellar_asset" }
            }},
        })),
    )];

    let deployments =
        extract_contract_deployments(&changes, "GDEPLOYER", &HashMap::new(), &HashMap::new());
    assert_eq!(deployments.len(), 1);
    assert!(deployments[0].is_sac);
    assert_eq!(deployments[0].contract_type, ContractType::Token);
    assert!(deployments[0].wasm_hash.is_none());
}

#[test]
fn skip_non_instance_contract_data() {
    let changes = vec![make_change(
        "contract_data",
        "created",
        json!({
            "contract": "CABC123",
            "key": { "type": "sym", "value": "counter" },
            "durability": "persistent",
        }),
        Some(json!({
            "contract": "CABC123",
            "key": { "type": "sym", "value": "counter" },
            "durability": "persistent",
            "val": { "type": "u64", "value": 42 },
        })),
    )];

    let deployments =
        extract_contract_deployments(&changes, "GDEPLOYER", &HashMap::new(), &HashMap::new());
    assert!(deployments.is_empty());
}

// -- Soroban token balance tests (task 0331) --

#[test]
fn extract_balance_entry_bare_i128() {
    let key = json!({
        "contract": "CTOKEN1",
        "key": { "type": "vec", "value": [
            { "type": "sym", "value": "Balance" },
            { "type": "address", "value": "GHOLDER1" }
        ]},
        "durability": "persistent",
    });
    let mut data = key.clone();
    data["val"] = json!({ "type": "i128", "value": "800009446178" });
    let changes = vec![make_change("contract_data", "updated", key, Some(data))];

    let balances = extract_soroban_token_balances(&changes);
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].contract_id, "CTOKEN1");
    assert_eq!(balances[0].holder, "GHOLDER1");
    assert_eq!(balances[0].balance, 800_009_446_178_i128);
    assert_eq!(balances[0].ledger, 100);
}

/// Real mainnet end-to-end (RPC `getLedgerEntries`, 2026-07-01): decodes the
/// ACTUAL `Balance(GAWOKP6N…)` ContractData entry for token `CCSNFZ5R…` at
/// ledger 63268948 from on-chain XDR, converts its real key + val `ScVal`s
/// via the REAL `scval_to_typed_json` (the exact JSON the ingestion emits),
/// and asserts the live parser recovers contract + holder + balance. The
/// i128 comes from the LEDGER BYTES (not a test constant) and equals the
/// independent `stellar contract invoke … balance` read (10000040000000) —
/// NON-circular.
#[test]
fn extract_balance_real_mainnet_entry() {
    use base64::Engine;
    use stellar_xdr::{LedgerEntryData, Limits, ReadXdr};

    let entry_b64 = "AAAABgAAAAAAAAABpNLnsQaIecmK0DuR3iIEA4DUoHpK2z+hSQS0L4ntArUAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAAAAAAALOU/zUgs2L4DJx225wMqTkYuiH78AX+HaE65g2akcB4AAAABAAAACgAAAAAAAAAAAAAJGFDU+gA=";
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(entry_b64)
        .unwrap();
    let LedgerEntryData::ContractData(entry) =
        LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap()
    else {
        panic!("expected ContractData");
    };

    let key = json!({
        "contract": entry.contract.to_string(),
        "key": crate::scval::scval_to_typed_json(&entry.key),
        "durability": "persistent",
    });
    let mut data = key.clone();
    data["val"] = crate::scval::scval_to_typed_json(&entry.val);

    let balances =
        extract_soroban_token_balances(&[make_change("contract_data", "updated", key, Some(data))]);

    assert_eq!(balances.len(), 1, "one balance from the real entry");
    assert_eq!(
        balances[0].contract_id,
        "CCSNFZ5RA2EHTSMK2A5ZDXRCAQBYBVFAPJFNWP5BJECLIL4J5UBLLUQG"
    );
    assert_eq!(
        balances[0].holder,
        "GAWOKP6NJAWNRPQDE4O3NZYDFJHEMLUIP36AC74HNBHLTA3GURYB4PYJ"
    );
    assert_eq!(
        balances[0].balance, 10_000_040_000_000,
        "parser must decode the exact on-chain i128 from the real entry"
    );
}

/// Real mainnet (RPC `getLedgerEntries`, 2026-07-01): the ACTUAL SAC `BalanceValue`
/// struct entries for the AMM pool `CATUJXDU…` holding native XLM and EURC (each held
/// via the asset's SAC — the contract-held classic/native case the type-3 bare-`i128`
/// path drops today). Decodes on-chain XDR → real `scval_to_typed_json` → the new
/// `decode_sac_balance_value`, and asserts the amount equals the INDEPENDENT
/// `stellar contract invoke … balance` read (native 11_635_129_310_963, EURC
/// 2_026_487_623_620) — the i128 comes from the ledger bytes, NON-circular. Also
/// asserts the bare-`i128` decoder rejects the struct (the two shapes never cross-decode).
#[test]
fn decode_sac_balance_value_real_mainnet() {
    use base64::Engine;
    use stellar_xdr::{LedgerEntryData, Limits, ReadXdr};

    for (entry_b64, expected_amount) in [
        (
            "AAAABgAAAAAAAAABJbT82FmuwvpjSEOMSJs8PBDJi20hvk/TyzDLaJU++XcAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAEnRNx0d+UpTAqVK9xT0ZTDwPQVQeA669CYruDz0/SWywAAAAEAAAARAAAAAQAAAAMAAAAPAAAABmFtb3VudAAAAAAACgAAAAAAAAAAAAAKlQO/3vMAAAAPAAAACmF1dGhvcml6ZWQAAAAAAAAAAAABAAAADwAAAAhjbGF3YmFjawAAAAAAAAAA",
            11_635_129_310_963_i128,
        ),
        (
            "AAAABgAAAAAAAAAB5qfZ63UjAGpGmqdIOtEQckdEPA2C5idj3mcISMTpfJAAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAEnRNx0d+UpTAqVK9xT0ZTDwPQVQeA669CYruDz0/SWywAAAAEAAAARAAAAAQAAAAMAAAAPAAAABmFtb3VudAAAAAAACgAAAAAAAAAAAAAB19QTL8QAAAAPAAAACmF1dGhvcml6ZWQAAAAAAAAAAAABAAAADwAAAAhjbGF3YmFjawAAAAAAAAAA",
            2_026_487_623_620_i128,
        ),
    ] {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(entry_b64)
            .unwrap();
        let LedgerEntryData::ContractData(entry) =
            LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap()
        else {
            panic!("expected ContractData");
        };
        let data = json!({ "val": crate::scval::scval_to_typed_json(&entry.val) });

        let decoded = decode_sac_balance_value(&data).expect("SAC BalanceValue decodes");
        assert_eq!(
            decoded.amount, expected_amount,
            "amount must equal the independent on-chain balance() read"
        );
        assert!(decoded.authorized, "pool balances are authorized");
        assert!(!decoded.clawback, "no clawback on these balances");

        // The two value shapes must never cross-decode.
        assert!(
            decode_scval_i128(&data).is_none(),
            "bare-i128 decoder must reject the SAC struct"
        );
    }
}

#[test]
fn decode_sac_balance_value_rejects_foreign_maps() {
    // Missing a required field (no `clawback`) → None.
    let missing = json!({ "val": { "type": "map", "value": [
        { "key": { "type": "sym", "value": "amount" }, "value": { "type": "i128", "value": "5" } },
        { "key": { "type": "sym", "value": "authorized" }, "value": { "type": "bool", "value": true } },
    ]}});
    assert!(decode_sac_balance_value(&missing).is_none());

    // An unknown extra symbol key → None (not the SAC struct, don't partial-decode).
    let foreign = json!({ "val": { "type": "map", "value": [
        { "key": { "type": "sym", "value": "amount" }, "value": { "type": "i128", "value": "5" } },
        { "key": { "type": "sym", "value": "authorized" }, "value": { "type": "bool", "value": true } },
        { "key": { "type": "sym", "value": "clawback" }, "value": { "type": "bool", "value": false } },
        { "key": { "type": "sym", "value": "extra" }, "value": { "type": "i128", "value": "9" } },
    ]}});
    assert!(decode_sac_balance_value(&foreign).is_none());

    // A bare i128 (type-3 shape) is not a map → None.
    let bare = json!({ "val": { "type": "i128", "value": "5" } });
    assert!(decode_sac_balance_value(&bare).is_none());
}

/// Contract-held SAC balance end-to-end (real mainnet entry): `extract_soroban_token_balances`
/// now emits the contract-held balance for a SAC `BalanceValue` struct — the
/// pool `CATUJXDU…` holding native XLM in the XLM SAC. Same real entry as
/// `decode_sac_balance_value_real_mainnet`, but asserts the WHOLE extractor
/// (not just the value decoder) surfaces it: contract = the SAC, holder = the
/// pool, balance = the independent on-chain read. Keyed downstream by the SAC
/// surrogate → lands on the SAC's type-2 asset row (task 0339 folds to type-0).
#[test]
fn extract_sac_struct_balance_real_mainnet() {
    use base64::Engine;
    use stellar_xdr::{LedgerEntryData, Limits, ReadXdr};

    let entry_b64 = "AAAABgAAAAAAAAABJbT82FmuwvpjSEOMSJs8PBDJi20hvk/TyzDLaJU++XcAAAAQAAAAAQAAAAIAAAAPAAAAB0JhbGFuY2UAAAAAEgAAAAEnRNx0d+UpTAqVK9xT0ZTDwPQVQeA669CYruDz0/SWywAAAAEAAAARAAAAAQAAAAMAAAAPAAAABmFtb3VudAAAAAAACgAAAAAAAAAAAAAKlQO/3vMAAAAPAAAACmF1dGhvcml6ZWQAAAAAAAAAAAABAAAADwAAAAhjbGF3YmFjawAAAAAAAAAA";
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(entry_b64)
        .unwrap();
    let LedgerEntryData::ContractData(entry) =
        LedgerEntryData::from_xdr(&bytes, Limits::none()).unwrap()
    else {
        panic!("expected ContractData");
    };
    let key = json!({
        "contract": entry.contract.to_string(),
        "key": crate::scval::scval_to_typed_json(&entry.key),
        "durability": "persistent",
    });
    let mut data = key.clone();
    data["val"] = crate::scval::scval_to_typed_json(&entry.val);

    let balances =
        extract_soroban_token_balances(&[make_change("contract_data", "updated", key, Some(data))]);

    assert_eq!(
        balances.len(),
        1,
        "extractor surfaces the SAC struct balance"
    );
    assert_eq!(
        balances[0].contract_id, "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA",
        "stored in the native XLM SAC"
    );
    assert_eq!(
        balances[0].holder, "CATUJXDUO7SSSTAKSUV5YU6RSTB4B5AVIHQDV26QTCXOB46T6SLMWNMY",
        "held by the pool contract"
    );
    assert_eq!(
        balances[0].balance, 11_635_129_310_963,
        "the exact on-chain SAC balance"
    );
}

#[test]
fn removed_balance_entry_emits_zero() {
    // Holder fully spent → entry removed (data is None). Must emit balance 0
    // so the RMT supersedes the stale positive balance (else over-count).
    let changes = vec![make_change(
        "contract_data",
        "removed",
        json!({
            "contract": "CTOKEN1",
            "key": { "type": "vec", "value": [
                { "type": "sym", "value": "Balance" },
                { "type": "address", "value": "GHOLDER1" }
            ]},
            "durability": "persistent",
        }),
        None,
    )];

    let balances = extract_soroban_token_balances(&changes);
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].holder, "GHOLDER1");
    assert_eq!(balances[0].balance, 0);
}

#[test]
fn skip_state_preimage_balance_entry() {
    // `state` is the pre-image (shares the change's ledger). Emitting it would
    // let the RMT clobber the real value with the old balance.
    let key = json!({
        "contract": "CTOKEN1",
        "key": { "type": "vec", "value": [
            { "type": "sym", "value": "Balance" },
            { "type": "address", "value": "GHOLDER1" }
        ]},
        "durability": "persistent",
    });
    let mut data = key.clone();
    data["val"] = json!({ "type": "i128", "value": "999" });
    let changes = vec![make_change("contract_data", "state", key, Some(data))];
    assert!(extract_soroban_token_balances(&changes).is_empty());
}

#[test]
fn skip_temporary_durability_balance_entry() {
    // Exact `Balance(Address)` shape but TEMPORARY durability — a real token /
    // SAC balance is PERSISTENT, so this foreign temp entry must be skipped.
    let key = json!({
        "contract": "CTOKEN1",
        "key": { "type": "vec", "value": [
            { "type": "sym", "value": "Balance" },
            { "type": "address", "value": "GHOLDER1" }
        ]},
        "durability": "temporary",
    });
    let mut data = key.clone();
    data["val"] = json!({ "type": "i128", "value": "999" });
    let changes = vec![make_change("contract_data", "updated", key, Some(data))];
    assert!(extract_soroban_token_balances(&changes).is_empty());
}

#[test]
fn skip_non_balance_contract_data_for_balances() {
    // Symbol("name") and instance keys must not be mistaken for a balance.
    let changes = vec![make_change(
        "contract_data",
        "updated",
        json!({
            "contract": "CTOKEN1",
            "key": { "type": "sym", "value": "name" },
            "durability": "persistent",
        }),
        Some(json!({
            "contract": "CTOKEN1",
            "key": { "type": "sym", "value": "name" },
            "durability": "persistent",
            "val": { "type": "string", "value": "MERU" },
        })),
    )];
    assert!(extract_soroban_token_balances(&changes).is_empty());
}

#[test]
fn skip_updated_contract_instance() {
    let changes = vec![make_change(
        "contract_data",
        "updated",
        json!({
            "contract": "CABC123",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
        }),
        Some(json!({
            "contract": "CABC123",
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
            "val": { "type": "contract_instance", "value": {
                "executable": { "type": "wasm", "hash": "bb".repeat(32) }
            }},
        })),
    )];

    let deployments =
        extract_contract_deployments(&changes, "GDEPLOYER", &HashMap::new(), &HashMap::new());
    assert!(deployments.is_empty());
}

// -- extract_contract_metadata_writes (task 0297 side-table source) --

/// Helper: a contract-instance change with a given executable type and a
/// preset `token_metadata` (mirrors what `ledger_entry_changes` produces).
fn make_instance_meta_change(
    contract_id: &str,
    change_type: &str,
    executable_type: &str,
    meta: crate::token_metadata::TokenMetadata,
) -> ExtractedLedgerEntryChange {
    let exec = if executable_type == "wasm" {
        json!({ "type": "wasm", "hash": "aa".repeat(32) })
    } else {
        json!({ "type": "stellar_asset" })
    };
    let mut c = make_change(
        "contract_data",
        change_type,
        json!({
            "contract": contract_id,
            "key": { "type": "ledger_key_contract_instance", "value": null },
            "durability": "persistent",
        }),
        Some(json!({ "val": { "type": "contract_instance", "value": { "executable": exec } } })),
    );
    // Mirror `entry_token_metadata`: a SAC instance yields no metadata.
    c.token_metadata = if executable_type == "stellar_asset" {
        None
    } else {
        Some(meta)
    };
    c
}

#[test]
fn extract_metadata_writes_from_wasm_instance() {
    let c = make_instance_meta_change(
        "CWASMTOKEN",
        "created",
        "wasm",
        crate::token_metadata::TokenMetadata {
            name: Some("liquidFi bridge token".into()),
            symbol: Some("lUSDC".into()),
            decimals: Some(7),
        },
    );
    let writes = extract_contract_metadata_writes(&[c]);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].contract_id, "CWASMTOKEN");
    assert_eq!(
        writes[0].metadata.name.as_deref(),
        Some("liquidFi bridge token")
    );
    assert_eq!(writes[0].metadata.symbol.as_deref(), Some("lUSDC"));
    assert_eq!(writes[0].metadata.decimals, Some(7));
    assert_eq!(writes[0].ledger, 100);
}

#[test]
fn extract_metadata_writes_skips_sac() {
    // SAC METADATA (name = CODE:ISSUER) is redundant with SAC identity → skip.
    let c = make_instance_meta_change(
        "CSAC",
        "created",
        "stellar_asset",
        crate::token_metadata::TokenMetadata {
            name: Some("USDC:GISSUER".into()),
            symbol: Some("USDC".into()),
            decimals: Some(7),
        },
    );
    assert!(extract_contract_metadata_writes(&[c]).is_empty());
}

#[test]
fn extract_metadata_writes_includes_restored() {
    // A contract instance restored from archival re-materializes the
    // current value — its METADATA must be (re)written (closes the
    // cold-start-after-eviction hole; task 0297 review).
    let c = make_instance_meta_change(
        "CRESTORED",
        "restored",
        "wasm",
        crate::token_metadata::TokenMetadata {
            name: Some("Restored Token".into()),
            symbol: Some("RST".into()),
            decimals: Some(7),
        },
    );
    let w = extract_contract_metadata_writes(&[c]);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].metadata.name.as_deref(), Some("Restored Token"));
}

#[test]
fn extract_metadata_writes_skips_state_preimage() {
    let c = make_instance_meta_change(
        "CWASM2",
        "state",
        "wasm",
        crate::token_metadata::TokenMetadata {
            name: Some("old".into()),
            symbol: None,
            decimals: None,
        },
    );
    assert!(extract_contract_metadata_writes(&[c]).is_empty());
}

#[test]
fn extract_metadata_writes_includes_updated_late_init() {
    let c = make_instance_meta_change(
        "CLATE",
        "updated",
        "wasm",
        crate::token_metadata::TokenMetadata {
            name: Some("Late Init Token".into()),
            symbol: Some("LI".into()),
            decimals: Some(6),
        },
    );
    let w = extract_contract_metadata_writes(&[c]);
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].metadata.name.as_deref(), Some("Late Init Token"));
    assert_eq!(w[0].metadata.decimals, Some(6));
}

// -- Account State Tests --

#[test]
fn extract_created_account_state() {
    let changes = vec![make_change(
        "account",
        "created",
        json!({ "account_id": "GABC123" }),
        Some(json!({
            "account_id": "GABC123",
            "balance": 1000000,
            "seq_num": 1,
            "home_domain": "",
            "num_sub_entries": 0,
            "thresholds": "01000000",
            "flags": 0,
        })),
    )];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_id, "GABC123");
    assert_eq!(accounts[0].sequence_number, 1);
    assert!(accounts[0].first_seen_ledger.is_some());
    assert!(accounts[0].home_domain.is_none()); // empty string filtered
}

#[test]
fn extract_updated_account_with_home_domain() {
    let changes = vec![make_change(
        "account",
        "updated",
        json!({ "account_id": "GABC123" }),
        Some(json!({
            "account_id": "GABC123",
            "balance": 5000000,
            "seq_num": 42,
            "home_domain": "example.com",
            "num_sub_entries": 2,
            "thresholds": "01000000",
            "flags": 0,
        })),
    )];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    assert!(accounts[0].first_seen_ledger.is_none());
    assert_eq!(accounts[0].home_domain.as_deref(), Some("example.com"));
    assert_eq!(accounts[0].sequence_number, 42);
}

#[test]
fn skip_state_only_account() {
    // `state` is a read-only pre-image snapshot; account state is derived
    // only from created/updated/restored (plus the removed tombstone
    // below). A lone `state` change yields nothing.
    let changes = vec![make_change(
        "account",
        "state",
        json!({}),
        Some(json!({"account_id": "G1", "balance": 0, "seq_num": 0})),
    )];

    assert!(extract_account_states(&changes).is_empty());
}

#[test]
fn removed_account_emits_zero_native_tombstone() {
    // AccountMerge: a `removed` account ⇒ native balance=0 tombstone at the
    // merge ledger, account_id from the key (removed carries no data).
    let changes = vec![make_change(
        "account",
        "removed",
        json!({ "account_id": "GMERGED" }),
        None,
    )];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let a = &accounts[0];
    assert_eq!(a.account_id, "GMERGED");
    assert_eq!(a.last_seen_ledger, 100); // merge ledger
    assert!(a.first_seen_ledger.is_none()); // not a creation
    assert_eq!(a.sequence_number, -1); // no seq on removal — must not clobber
    assert_eq!(a.balances[0]["asset_type"], "native");
    assert_eq!(a.balances[0]["balance"], "0.0000000");
    // The 0 alone cannot say "this account is gone" — an account holding no
    // XLM writes the same value and is very much alive (CAP-0033 sponsored
    // reserves). ADR 0055.
    assert!(a.account_removed, "the merge must be marked as a closure");
}

#[test]
fn live_account_holding_no_xlm_is_not_marked_removed() {
    // The counter-case that makes the flag worth having: balance 0, account
    // alive. Measured at 4.27M zero-native rows in production, of which
    // 239,087 sit alongside a positive non-native balance.
    let changes = vec![make_change(
        "account",
        "updated",
        json!({ "account_id": "GPOOR" }),
        Some(json!({
            "account_id": "GPOOR",
            "balance": 0,
            "seq_num": 7,
            "home_domain": "",
        })),
    )];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts[0].balances[0]["balance"], "0.0000000");
    assert!(
        !accounts[0].account_removed,
        "a live account at zero XLM must not read as merged"
    );
}

/// lore-0463: signers/thresholds/flags flow through the accumulator with
/// full-set semantics — Some(empty) is a real state, None means the entry
/// was never observed.
#[test]
fn signers_flow_through_with_full_set_semantics() {
    let changes = vec![make_change(
        "account",
        "updated",
        json!({ "account_id": "GMULTI" }),
        Some(json!({
            "account_id": "GMULTI",
            "balance": 100,
            "seq_num": 5,
            "home_domain": "",
            "thresholds": "01030303",
            "flags": 4,
            "signers": [
                {"key": "GS1", "weight": 1, "type": "ed25519"},
                {"key": "TS2", "weight": 2, "type": "preauth_tx"},
            ],
        })),
    )];
    let a = &extract_account_states(&changes)[0];
    assert_eq!(a.thresholds.as_deref(), Some("01030303"));
    assert_eq!(a.flags, Some(4));
    let sg = a.signers.as_ref().expect("entry observed => Some");
    assert_eq!(sg.len(), 2);
    assert_eq!(sg[1]["type"], "preauth_tx");
}

#[test]
fn entry_without_signers_field_yields_some_empty_not_none() {
    // Removing the last signer emits an entry whose set is empty — that
    // MUST surface as Some(empty), or persist would skip the write and
    // the stale set would survive in the RMT forever.
    let changes = vec![make_change(
        "account",
        "updated",
        json!({ "account_id": "GBARE" }),
        Some(json!({
            "account_id": "GBARE",
            "balance": 1,
            "seq_num": 1,
            "home_domain": "",
            "thresholds": "01000000",
            "flags": 0,
        })),
    )];
    let a = &extract_account_states(&changes)[0];
    assert_eq!(a.signers.as_deref(), Some(&[][..]));
    assert_eq!(a.thresholds.as_deref(), Some("01000000"));
}

#[test]
fn trustline_only_change_never_observes_signers() {
    let changes = vec![make_change(
        "trustline",
        "created",
        json!({ "account_id": "GTL" }),
        Some(json!({
            "account_id": "GTL",
            "asset": {"type": "credit_alphanum4", "code": "AQUA", "issuer": "GISS"},
            "balance": 5,
            "limit": 100,
            "flags": 1,
        })),
    )];
    let a = &extract_account_states(&changes)[0];
    assert!(
        a.signers.is_none() && a.thresholds.is_none(),
        "a trustline-only accum must not fabricate an observed entry"
    );
}

#[test]
fn merge_then_recreate_in_one_change_set_is_not_closed() {
    // Order matters: the removal is seen first, a live entry follows. The
    // account exists at the end of the ledger, so the closure must be
    // cancelled — otherwise the read path would hide a live account.
    let changes = vec![
        make_change(
            "account",
            "removed",
            json!({ "account_id": "GPHOENIX" }),
            None,
        ),
        make_change(
            "account",
            "created",
            json!({ "account_id": "GPHOENIX" }),
            Some(json!({
                "account_id": "GPHOENIX",
                "balance": 50_000_000,
                "seq_num": 1,
                "home_domain": "",
            })),
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    assert!(
        !accounts[0].account_removed,
        "recreated in the same change set — must not stay marked closed"
    );
    assert_eq!(accounts[0].balances[0]["balance"], "5.0000000");
}

// -- Trustline Balance Tests (0119) --

#[test]
fn account_with_two_trustlines() {
    let changes = vec![
        make_change(
            "account",
            "created",
            json!({ "account_id": "GABC" }),
            Some(json!({
                "account_id": "GABC",
                "balance": 1000000,
                "seq_num": 1,
                "home_domain": "",
                "num_sub_entries": 2,
                "thresholds": "01000000",
                "flags": 0,
            })),
        ),
        make_change(
            "trustline",
            "created",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER1" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER1" },
                "balance": 5000,
                "limit": 10000,
                "flags": 1,
            })),
        ),
        make_change(
            "trustline",
            "created",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum12", "code": "EUROC", "issuer": "GISSUER2" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum12", "code": "EUROC", "issuer": "GISSUER2" },
                "balance": 3000,
                "limit": 50000,
                "flags": 1,
            })),
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let a = &accounts[0];
    assert_eq!(a.account_id, "GABC");
    assert_eq!(a.sequence_number, 1);
    assert!(a.first_seen_ledger.is_some());
    let balances = a.balances.as_array().unwrap();
    assert_eq!(balances.len(), 3);
    assert!(
        balances
            .iter()
            .any(|b| b["asset_type"] == "native" && b["balance"] == "0.1000000")
    );
    assert!(
        balances
            .iter()
            .any(|b| b["asset_code"] == "USDC" && b["balance"] == "0.0005000")
    );
    assert!(
        balances
            .iter()
            .any(|b| b["asset_code"] == "EUROC" && b["balance"] == "0.0003000")
    );
    assert!(a.removed_trustlines.is_empty());
}

#[test]
fn trustline_only_change() {
    let changes = vec![make_change(
        "trustline",
        "updated",
        json!({
            "account_id": "GABC",
            "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER1" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER1" },
            "balance": 9999,
            "limit": 10000,
            "flags": 1,
        })),
    )];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let a = &accounts[0];
    assert_eq!(a.sequence_number, -1); // sentinel
    let balances = a.balances.as_array().unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0]["asset_code"], "USDC");
    assert_eq!(balances[0]["balance"], "0.0009999");
}

#[test]
fn trustline_removal() {
    let changes = vec![
        make_change(
            "account",
            "updated",
            json!({ "account_id": "GABC" }),
            Some(json!({
                "account_id": "GABC",
                "balance": 500,
                "seq_num": 10,
                "home_domain": "",
                "num_sub_entries": 0,
                "thresholds": "01000000",
                "flags": 0,
            })),
        ),
        make_change(
            "trustline",
            "removed",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER1" },
            }),
            None,
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let a = &accounts[0];
    assert_eq!(a.sequence_number, 10);
    let balances = a.balances.as_array().unwrap();
    assert_eq!(balances.len(), 1); // only native remains
    assert_eq!(balances[0]["asset_type"], "native");
    assert_eq!(a.removed_trustlines.len(), 1);
    assert_eq!(a.removed_trustlines[0]["asset_code"], "USDC");
}

#[test]
fn trustline_update_dedup() {
    let changes = vec![
        make_change(
            "trustline",
            "updated",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
                "balance": 100,
                "limit": 10000,
                "flags": 1,
            })),
        ),
        make_change(
            "trustline",
            "updated",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
                "balance": 200,
                "limit": 10000,
                "flags": 1,
            })),
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let balances = accounts[0].balances.as_array().unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0]["balance"], "0.0000200"); // last wins
}

#[test]
fn pool_share_trustline_skipped() {
    let changes = vec![make_change(
        "trustline",
        "created",
        json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
            "balance": 1000,
            "limit": 99999,
            "flags": 0,
        })),
    )];

    let accounts = extract_account_states(&changes);
    assert!(accounts.is_empty());
}

// -- Task 0162: extract_lp_positions --

#[test]
fn lp_position_extracted_from_created_pool_share_trustline() {
    let changes = vec![make_change(
        "trustline",
        "created",
        json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
            "balance": 420_000_000_i64,  // 42 shares in stroops
            "limit": 99_999_999_999_i64,
            "flags": 0,
        })),
    )];

    let positions = extract_lp_positions(&changes);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].pool_id, "aabb");
    assert_eq!(positions[0].account_id, "GABC");
    assert_eq!(positions[0].shares, "42.0000000");
    assert_eq!(positions[0].first_deposit_ledger, Some(100));
    assert_eq!(positions[0].last_updated_ledger, 100);
}

#[test]
fn lp_position_updated_drops_first_deposit_ledger() {
    let changes = vec![make_change(
        "trustline",
        "updated",
        json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
            "balance": 50_000_000_i64,
            "limit": 99_999_999_999_i64,
            "flags": 0,
        })),
    )];

    let positions = extract_lp_positions(&changes);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].shares, "5.0000000");
    // updated → preserve original first_deposit_ledger via NULL +
    // staging COALESCE, not overwrite from this change.
    assert!(positions[0].first_deposit_ledger.is_none());
}

#[test]
fn lp_position_removed_emits_zero_shares_from_key() {
    // `removed` change has no `data`; account_id + asset come from `key`.
    let changes = vec![make_change(
        "trustline",
        "removed",
        json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
        }),
        None,
    )];

    let positions = extract_lp_positions(&changes);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].shares, "0.0000000");
    assert!(positions[0].first_deposit_ledger.is_none());
    assert_eq!(positions[0].last_updated_ledger, 100);
}

#[test]
fn lp_positions_ignore_credit_trustlines() {
    // Regular credit trustline must not produce an LP position;
    // account-state path handles it instead.
    let changes = vec![make_change(
        "trustline",
        "created",
        json!({
            "account_id": "GABC",
            "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "GISSUER" },
            "balance": 5_000_000_i64,
            "limit": 99_999_999_999_i64,
            "flags": 0,
        })),
    )];

    assert!(extract_lp_positions(&changes).is_empty());
    // The same change does still contribute to account state.
    assert_eq!(extract_account_states(&changes).len(), 1);
}

#[test]
fn lp_positions_ignore_state_change_type() {
    // `state` is observation-only (no balance change) — do not emit.
    let changes = vec![make_change(
        "trustline",
        "state",
        json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
        }),
        Some(json!({
            "account_id": "GABC",
            "asset": { "type": "pool_share", "pool_id": "aabb" },
            "balance": 100_000_000_i64,
            "limit": 99_999_999_999_i64,
            "flags": 0,
        })),
    )];

    assert!(extract_lp_positions(&changes).is_empty());
}

// -- Lore-0189: a pool seen only at its removal keeps its dimension row --

#[test]
fn pool_seen_only_at_its_removal_keeps_its_row() {
    // Lore-0189 reproducer, re-read on 2026-09-17 (task 0210): ledger 62148003
    // carries pool d63184… as `state` + `removed` in a `change_trust`, not as a
    // lone read-only `state` as first assumed. The pool row comes from the
    // `state` params, so the `lp_positions` row of that ledger still resolves;
    // the snapshot is zero, because the pool no longer exists.
    let pool_id = "d63184d4e5601fad174d9d5fa8e79f2366f6818892e43867a952e8adb13fa561";
    let changes = vec![
        make_change(
            "liquidity_pool",
            "state",
            json!({ "pool_id": pool_id }),
            Some(json!({
                "pool_id": pool_id,
                "params": {
                    "asset_a": { "type": "credit_alphanum4", "code": "Lira", "issuer": "GBU3EGQO" },
                    "asset_b": { "type": "credit_alphanum12", "code": "liragold", "issuer": "GAIHDHWF" },
                    "fee": 30,
                },
                "reserve_a": 0,
                "reserve_b": 0,
                "total_pool_shares": 0,
                "type": "constant_product",
            })),
        ),
        lp_removed(pool_id),
    ];

    let (pools, snapshots) = extract_liquidity_pools(&changes);
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0].pool_id, pool_id);
    assert_eq!(pools[0].fee_bps, 30);
    assert_eq!(pools[0].asset_a["code"], "Lira");
    assert!(
        pools[0].created_at_ledger.is_none(),
        "a removal is not a creation"
    );
    assert_eq!(pools[0].last_updated_ledger, 100);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].reserves, json!({ "a": 0, "b": 0 }));
}

#[test]
fn removal_cancels_same_tx_creation() {
    let changes = vec![
        make_change(
            "trustline",
            "created",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
                "balance": 500,
                "limit": 10000,
                "flags": 1,
            })),
        ),
        make_change(
            "trustline",
            "removed",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            None,
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let balances = accounts[0].balances.as_array().unwrap();
    assert!(balances.is_empty()); // creation was cancelled by removal
    assert_eq!(accounts[0].removed_trustlines.len(), 1);
}

#[test]
fn recreate_cancels_prior_removal_same_tx() {
    let changes = vec![
        // First: trustline removed
        make_change(
            "trustline",
            "removed",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            None,
        ),
        // Then: trustline re-created
        make_change(
            "trustline",
            "created",
            json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
            }),
            Some(json!({
                "account_id": "GABC",
                "asset": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G1" },
                "balance": 700,
                "limit": 10000,
                "flags": 1,
            })),
        ),
    ];

    let accounts = extract_account_states(&changes);
    assert_eq!(accounts.len(), 1);
    let balances = accounts[0].balances.as_array().unwrap();
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0]["asset_code"], "USDC");
    assert_eq!(balances[0]["balance"], "0.0000700");
    // Removal should be cancelled — trustline was re-created
    assert!(accounts[0].removed_trustlines.is_empty());
}

// -- Liquidity Pool Tests --

#[test]
fn extract_pool_produces_state_and_snapshot() {
    let changes = vec![make_change(
        "liquidity_pool",
        "created",
        json!({ "pool_id": "aabb" }),
        Some(json!({
            "pool_id": "aabb",
            "type": "constant_product",
            "params": {
                "asset_a": "native",
                "asset_b": { "type": "credit_alphanum4", "code": "USDC", "issuer": "G..." },
                "fee": 30,
            },
            "reserve_a": 10000,
            "reserve_b": 20000,
            "total_pool_shares": 5000,
            "pool_shares_trust_line_count": 3,
        })),
    )];

    let (pools, snapshots) = extract_liquidity_pools(&changes);
    assert_eq!(pools.len(), 1);
    assert_eq!(snapshots.len(), 1);

    assert_eq!(pools[0].pool_id, "aabb");
    assert_eq!(pools[0].fee_bps, 30);
    assert!(pools[0].created_at_ledger.is_some());
    assert_eq!(pools[0].total_shares, "5000");

    assert_eq!(snapshots[0].pool_id, "aabb");
    assert_eq!(snapshots[0].reserves["a"], 10000);
    assert_eq!(snapshots[0].reserves["b"], 20000);
}

// -- Asset Detection Tests --

use crate::types::ContractFunction;

fn iface(wasm_hash: &str, fn_names: &[&str]) -> ExtractedContractInterface {
    ExtractedContractInterface {
        wasm_hash: wasm_hash.to_string(),
        functions: fn_names
            .iter()
            .map(|n| ContractFunction {
                name: (*n).to_string(),
                doc: String::new(),
                inputs: Vec::new(),
                outputs: Vec::new(),
            })
            .collect(),
        wasm_byte_len: 0,
        upgradeable: false,
    }
}

#[test]
fn sac_credit_deployment_produces_classic_credit_with_sac_facet() {
    // ADR 0051: a SAC credit deploy folds onto the classic_credit row —
    // the SAC handle rides in `sac_contract_id` (+ `sac_deployed = true`),
    // NOT a separate `asset_type`; the key `contract_id` stays unset.
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CSAC456".into(),
        wasm_hash: None,
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Token,
        is_sac: true,
        sac_asset: Some(SacAssetIdentity::Credit {
            code: "USDC".into(),
            issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
        }),
    }];

    let assets = detect_assets(&deployments, &[]);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_type, AssetFamily::ClassicCredit);
    assert_eq!(assets[0].contract_id, None);
    assert_eq!(assets[0].sac_contract_id.as_deref(), Some("CSAC456"));
    assert!(assets[0].sac_deployed);
    // Task 0160 regression: SAC identity must survive through to the asset row.
    assert_eq!(assets[0].asset_code.as_deref(), Some("USDC"));
    assert_eq!(
        assets[0].issuer_address.as_deref(),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
    );
}

#[test]
fn sac_native_deployment_produces_native_with_sac_facet() {
    // ADR 0051: a SAC deploy wrapping native XLM folds onto the native
    // (type=0) row — NULL code/issuer, SAC handle in `sac_contract_id`.
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CXLM_SAC".into(),
        wasm_hash: None,
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Token,
        is_sac: true,
        sac_asset: Some(SacAssetIdentity::Native),
    }];

    let assets = detect_assets(&deployments, &[]);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_type, AssetFamily::Native);
    assert_eq!(assets[0].contract_id, None);
    assert_eq!(assets[0].sac_contract_id.as_deref(), Some("CXLM_SAC"));
    assert!(assets[0].sac_deployed);
    assert!(assets[0].asset_code.is_none());
    assert!(assets[0].issuer_address.is_none());
}

#[test]
fn sac_deployment_without_identity_is_skipped() {
    // SAC deployment whose creating preimage isn't in the current
    // batch (replay from mid-ledger). No asset row produced —
    // better to lose one row than fabricate identity.
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CORPHAN".into(),
        wasm_hash: None,
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Token,
        is_sac: true,
        sac_asset: None,
    }];

    let assets = detect_assets(&deployments, &[]);
    assert!(assets.is_empty());
}

#[test]
fn non_sac_without_interface_is_skipped() {
    // No matching interface in this batch → skip; late-WASM bridge
    // in persist layer handles reclassification/backfill.
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CABC123".into(),
        wasm_hash: Some("aa".repeat(32)),
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    }];

    let assets = detect_assets(&deployments, &[]);
    assert!(assets.is_empty());
}

#[test]
fn fungible_wasm_deployment_produces_soroban_asset() {
    // SEP-0041 surface → ContractClassification::Fungible → Soroban asset row.
    let wasm = "aa".repeat(32);
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CFUN001".into(),
        wasm_hash: Some(wasm.clone()),
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Fungible,
        is_sac: false,
        sac_asset: None,
    }];
    let interfaces = vec![iface(
        &wasm,
        &["transfer", "balance", "decimals", "name", "symbol"],
    )];

    let assets = detect_assets(&deployments, &interfaces);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_type, AssetFamily::Soroban);
    assert_eq!(assets[0].contract_id.as_deref(), Some("CFUN001"));
    assert!(assets[0].asset_code.is_none());
    assert!(assets[0].issuer_address.is_none());
}

#[test]
fn nft_wasm_deployment_produces_no_asset() {
    // NFT-classified contracts live in the `nfts` table, not `assets`.
    let wasm = "bb".repeat(32);
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CNFT002".into(),
        wasm_hash: Some(wasm.clone()),
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Nft,
        is_sac: false,
        sac_asset: None,
    }];
    let interfaces = vec![iface(&wasm, &["owner_of", "token_uri", "transfer"])];

    let assets = detect_assets(&deployments, &interfaces);
    assert!(assets.is_empty());
}

#[test]
fn other_wasm_deployment_produces_no_asset() {
    // Unknown contract surface — no asset row; a later WASM upload
    // may promote it via reclassify_contracts_from_wasm.
    let wasm = "cc".repeat(32);
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "COTH003".into(),
        wasm_hash: Some(wasm.clone()),
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    }];
    let interfaces = vec![iface(&wasm, &["execute", "admin", "init"])];

    let assets = detect_assets(&deployments, &interfaces);
    assert!(assets.is_empty());
}

#[test]
fn dual_interface_contract_produces_no_asset_row() {
    // Precedence in classify_contract_from_wasm_spec: NFT wins over
    // Fungible when both discriminators present. Correct downstream
    // behaviour: the contract goes to `nfts` filter — NOT `assets`.
    let wasm = "dd".repeat(32);
    let deployments = vec![ExtractedContractDeployment {
        executable_ref: None,
        contract_id: "CDUAL04".into(),
        wasm_hash: Some(wasm.clone()),
        deployer_account: None,
        deployed_at_ledger: 100,
        contract_type: ContractType::Nft,
        is_sac: false,
        sac_asset: None,
    }];
    let interfaces = vec![iface(&wasm, &["owner_of", "decimals", "transfer"])];

    let assets = detect_assets(&deployments, &interfaces);
    assert!(assets.is_empty());
}

#[test]
fn sac_and_fungible_in_same_batch_both_produce_assets() {
    let wasm = "ee".repeat(32);
    let deployments = vec![
        ExtractedContractDeployment {
            executable_ref: None,
            contract_id: "CSAC005".into(),
            wasm_hash: None,
            deployer_account: None,
            deployed_at_ledger: 100,
            contract_type: ContractType::Token,
            is_sac: true,
            sac_asset: Some(SacAssetIdentity::Credit {
                code: "USDC".into(),
                issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into(),
            }),
        },
        ExtractedContractDeployment {
            executable_ref: None,
            contract_id: "CFUN006".into(),
            wasm_hash: Some(wasm.clone()),
            deployer_account: None,
            deployed_at_ledger: 100,
            contract_type: ContractType::Fungible,
            is_sac: false,
            sac_asset: None,
        },
    ];
    let interfaces = vec![iface(&wasm, &["transfer", "decimals", "allowance"])];

    let assets = detect_assets(&deployments, &interfaces);
    assert_eq!(assets.len(), 2);
    // The SAC folds onto a classic_credit carrier (handle in `sac_contract_id`,
    // key `contract_id` unset); the bespoke fungible is a soroban row keyed by
    // its own `contract_id`.
    let sac = assets
        .iter()
        .find(|a| a.sac_contract_id.as_deref() == Some("CSAC005"))
        .expect("SAC carrier present");
    assert_eq!(sac.asset_type, AssetFamily::ClassicCredit);
    assert_eq!(sac.contract_id, None);
    assert!(sac.sac_deployed);
    let fungible = assets
        .iter()
        .find(|a| a.contract_id.as_deref() == Some("CFUN006"))
        .expect("soroban fungible present");
    assert_eq!(fungible.asset_type, AssetFamily::Soroban);
    assert_eq!(fungible.sac_contract_id, None);
}

// -- NFT Detection Tests --

#[test]
fn nft_mint_event_produces_nft() {
    let events = vec![NftEvent {
        transaction_hash: "abc".into(),
        contract_id: "CNFT789".into(),
        event_kind: "mint".into(),
        token_id: json!({"type": "u32", "value": 42}),
        from: None,
        to: Some("GOWNER".into()),
        ledger_sequence: 100,
        created_at: 1700000000,
    }];

    let nfts = detect_nfts(&events);
    assert_eq!(nfts.len(), 1);
    assert_eq!(nfts[0].contract_id, "CNFT789");
    assert_eq!(nfts[0].token_id, "42");
    assert_eq!(nfts[0].owner_account.as_deref(), Some("GOWNER"));
    assert_eq!(nfts[0].minted_at_ledger, Some(100));
}

#[test]
fn nft_transfer_event() {
    let events = vec![NftEvent {
        transaction_hash: "abc".into(),
        contract_id: "CNFT789".into(),
        event_kind: "transfer".into(),
        token_id: json!({"type": "u32", "value": 42}),
        from: Some("GFROM".into()),
        to: Some("GTO".into()),
        ledger_sequence: 200,
        created_at: 1700001000,
    }];

    let nfts = detect_nfts(&events);
    assert_eq!(nfts.len(), 1);
    assert_eq!(nfts[0].owner_account.as_deref(), Some("GTO"));
    assert!(nfts[0].minted_at_ledger.is_none());
}

#[test]
fn nft_burn_event() {
    let events = vec![NftEvent {
        transaction_hash: "abc".into(),
        contract_id: "CNFT789".into(),
        event_kind: "burn".into(),
        token_id: json!({"type": "string", "value": "unique-nft-id"}),
        from: Some("GFROM".into()),
        to: None,
        ledger_sequence: 300,
        created_at: 1700002000,
    }];

    let nfts = detect_nfts(&events);
    assert_eq!(nfts.len(), 1);
    assert_eq!(nfts[0].token_id, "unique-nft-id");
    assert!(nfts[0].owner_account.is_none());
}

#[test]
fn empty_token_id_skipped() {
    let events = vec![NftEvent {
        transaction_hash: "abc".into(),
        contract_id: "CNFT789".into(),
        event_kind: "mint".into(),
        token_id: json!({"type": "void", "value": null}),
        from: None,
        to: Some("GOWNER".into()),
        ledger_sequence: 100,
        created_at: 1700000000,
    }];

    let nfts = detect_nfts(&events);
    assert!(nfts.is_empty());
}

// -- NFT Ownership Event Extraction Tests (task 0202) --

fn make_nft_event(
    contract: &str,
    kind: &str,
    token: i64,
    from: Option<&str>,
    to: Option<&str>,
    ledger: u32,
) -> NftEvent {
    NftEvent {
        transaction_hash: format!("tx{}", ledger),
        contract_id: contract.into(),
        event_kind: kind.into(),
        token_id: json!({"type": "u32", "value": token}),
        from: from.map(Into::into),
        to: to.map(Into::into),
        ledger_sequence: ledger,
        created_at: 1700000000 + ledger as i64,
    }
}

#[test]
fn mint_event_yields_owner_to() {
    let events = vec![make_nft_event(
        "CNFT1",
        "mint",
        42,
        None,
        Some("GRECIPIENT"),
        100,
    )];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].contract_id, "CNFT1");
    assert_eq!(out[0].token_id, "42");
    assert_eq!(out[0].event_type, NftEventType::Mint);
    assert_eq!(out[0].owner_account.as_deref(), Some("GRECIPIENT"));
    assert_eq!(out[0].event_order, 0);
    assert_eq!(out[0].ledger_sequence, 100);
}

#[test]
fn transfer_event_yields_owner_to() {
    let events = vec![make_nft_event(
        "CNFT1",
        "transfer",
        42,
        Some("GFROM"),
        Some("GTO"),
        100,
    )];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].event_type, NftEventType::Transfer);
    assert_eq!(out[0].owner_account.as_deref(), Some("GTO"));
}

#[test]
fn burn_event_yields_owner_none() {
    let events = vec![make_nft_event(
        "CNFT1",
        "burn",
        42,
        Some("GBURNER"),
        None,
        100,
    )];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].event_type, NftEventType::Burn);
    assert!(out[0].owner_account.is_none());
}

#[test]
fn event_order_monotonic_per_triple() {
    let events = vec![
        make_nft_event("CNFT1", "mint", 42, None, Some("GA"), 100),
        make_nft_event("CNFT1", "transfer", 42, Some("GA"), Some("GB"), 100),
        make_nft_event("CNFT1", "transfer", 42, Some("GB"), Some("GC"), 100),
    ];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 3);
    assert_eq!(out[0].event_order, 0);
    assert_eq!(out[1].event_order, 1);
    assert_eq!(out[2].event_order, 2);
}

#[test]
fn event_order_resets_per_token() {
    let events = vec![
        // Same contract, same ledger, different tokens.
        make_nft_event("CNFT1", "mint", 42, None, Some("GA"), 100),
        make_nft_event("CNFT1", "mint", 43, None, Some("GB"), 100),
        // Different contract, same ledger.
        make_nft_event("CNFT2", "mint", 42, None, Some("GC"), 100),
    ];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 3);
    // Each (contract, token, ledger) triple starts its own counter.
    assert_eq!(out[0].event_order, 0);
    assert_eq!(out[1].event_order, 0);
    assert_eq!(out[2].event_order, 0);
}

#[test]
fn token_id_jsonvalue_stringified() {
    // Numeric token_id → "42".
    let numeric = NftEvent {
        transaction_hash: "tx1".into(),
        contract_id: "CNFT1".into(),
        event_kind: "mint".into(),
        token_id: json!({"type": "u64", "value": 42}),
        from: None,
        to: Some("GA".into()),
        ledger_sequence: 100,
        created_at: 1700000000,
    };
    // String token_id → "uuid-abc".
    let string = NftEvent {
        transaction_hash: "tx2".into(),
        contract_id: "CNFT1".into(),
        event_kind: "mint".into(),
        token_id: json!({"type": "string", "value": "uuid-abc"}),
        from: None,
        to: Some("GB".into()),
        ledger_sequence: 100,
        created_at: 1700000000,
    };

    let out = extract_nft_ownership_events(&[numeric, string]);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].token_id, "42");
    assert_eq!(out[1].token_id, "uuid-abc");
}

#[test]
fn empty_token_id_event_skipped() {
    // `token_id_to_string` returns "" when the JSON value is null
    // (e.g. type=void). Such events must be skipped so they never
    // reach staging — matches `detect_nfts` behaviour.
    let events = vec![NftEvent {
        transaction_hash: "tx1".into(),
        contract_id: "CNFT1".into(),
        event_kind: "mint".into(),
        token_id: json!({"type": "void", "value": null}),
        from: None,
        to: Some("GA".into()),
        ledger_sequence: 100,
        created_at: 1700000000,
    }];

    let out = extract_nft_ownership_events(&events);
    assert!(out.is_empty(), "event with empty token_id must be skipped");
}

#[test]
fn unknown_event_kind_skipped() {
    // Parser is supposed to emit only mint/transfer/burn; anything
    // else is a defence-in-depth skip path. Mixed batch must keep
    // the recognised events and drop the unknown one.
    let events = vec![
        make_nft_event("CNFT1", "approve", 42, Some("GA"), Some("GB"), 100),
        make_nft_event("CNFT1", "mint", 43, None, Some("GA"), 100),
    ];
    let out = extract_nft_ownership_events(&events);

    assert_eq!(out.len(), 1, "unknown event_kind must be skipped");
    assert_eq!(out[0].event_type, NftEventType::Mint);
    assert_eq!(out[0].token_id, "43");
}

#[test]
fn event_order_overflow_skips_excess_events() {
    // Pathological-input guard: once a (contract, token, ledger)
    // triple has emitted i16::MAX events, further events for that
    // triple are skipped with a warn rather than overflowing the
    // SMALLINT column at staging.
    const OVERFLOW_AT: u16 = i16::MAX as u16;

    let mut events = Vec::with_capacity((OVERFLOW_AT as usize) + 5);
    for _ in 0..(OVERFLOW_AT as usize + 5) {
        events.push(make_nft_event(
            "CNFT1",
            "transfer",
            42,
            Some("GA"),
            Some("GB"),
            100,
        ));
    }
    let out = extract_nft_ownership_events(&events);

    // Emits exactly i16::MAX + 1 rows (event_order 0..=32_767),
    // then refuses to write more — five excess events dropped.
    assert_eq!(
        out.len(),
        OVERFLOW_AT as usize + 1,
        "should emit one row per slot 0..=i16::MAX, no overflow"
    );
    assert_eq!(out.first().unwrap().event_order, 0);
    assert_eq!(out.last().unwrap().event_order, i16::MAX as u16);
}

// ----------------------------------------------------------------------
// Task 0219 — detect_classic_credit_assets + native_asset_singleton
// ----------------------------------------------------------------------

fn trustline_change(change_type: &str, code: &str, issuer: &str) -> ExtractedLedgerEntryChange {
    make_change(
        "trustline",
        change_type,
        json!({
            "account_id": "GHOLDER",
            "asset": { "type": "credit_alphanum4", "code": code, "issuer": issuer },
        }),
        Some(json!({
            "account_id": "GHOLDER",
            "asset": { "type": "credit_alphanum4", "code": code, "issuer": issuer },
            "balance": 10_000_000,
            "limit": 1_000_000_000,
        })),
    )
}

#[test]
fn classic_credit_assets_emitted_from_trustline_created() {
    let changes = vec![trustline_change(
        "created",
        "USDC",
        "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
    )];
    let assets = detect_classic_credit_assets(&changes);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_type, AssetFamily::ClassicCredit);
    assert_eq!(assets[0].asset_code.as_deref(), Some("USDC"));
    assert_eq!(
        assets[0].issuer_address.as_deref(),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
    );
    assert!(assets[0].contract_id.is_none());
}

#[test]
fn classic_credit_emitted_for_updated_restored_state_changes() {
    let issuer = "GISSUER";
    let changes = vec![
        trustline_change("created", "AQUA", issuer),
        trustline_change("updated", "AQUA", issuer),
        trustline_change("restored", "AQUA", issuer),
        trustline_change("state", "AQUA", issuer),
    ];
    let assets = detect_classic_credit_assets(&changes);
    // Same (code, issuer) across 4 change types → 1 row after dedup.
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_code.as_deref(), Some("AQUA"));
}

#[test]
fn classic_credit_removed_trustlines_use_key_asset_fallback() {
    // Removed trustlines carry `data: None`, but the change's
    // `key.asset` still holds `{type, code, issuer}`. A
    // partial-window backfill whose first observation of a
    // `(code, issuer)` pair is the trustline removal should still
    // emit the asset row — falling back to `key` covers that case.
    let mut change = trustline_change(
        "removed",
        "USDC",
        "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
    );
    change.data = None;
    let assets = detect_classic_credit_assets(&[change]);
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].asset_code.as_deref(), Some("USDC"));
    assert_eq!(
        assets[0].issuer_address.as_deref(),
        Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN")
    );
}

#[test]
fn classic_credit_removed_trustline_without_key_asset_is_skipped() {
    // Truly key-less removed change (key carries account_id only) —
    // we have no asset identity to emit; skip safely.
    let mut change = trustline_change("removed", "USDC", "GISSUER");
    change.data = None;
    change.key = json!({"account_id": "GHOLDER"}); // no `asset` field
    let assets = detect_classic_credit_assets(&[change]);
    assert!(assets.is_empty());
}

#[test]
fn classic_credit_dedups_same_code_issuer_across_changes() {
    let issuer = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    let changes = vec![
        trustline_change("created", "USDC", issuer),
        trustline_change("updated", "USDC", issuer),
        trustline_change("created", "EURC", issuer),
    ];
    let assets = detect_classic_credit_assets(&changes);
    // USDC dedups across two changes; EURC stands alone → 2 rows.
    assert_eq!(assets.len(), 2);
    let mut codes: Vec<_> = assets
        .iter()
        .map(|a| a.asset_code.as_deref().unwrap_or(""))
        .collect();
    codes.sort();
    assert_eq!(codes, vec!["EURC", "USDC"]);
}

#[test]
fn classic_credit_skips_pool_share_trustlines() {
    let change = make_change(
        "trustline",
        "created",
        json!({"account_id": "GHOLDER", "asset": {"type": "pool_share"}}),
        Some(json!({
            "account_id": "GHOLDER",
            "asset": { "type": "pool_share" },
            "balance": 1_000,
        })),
    );
    let assets = detect_classic_credit_assets(&[change]);
    assert!(
        assets.is_empty(),
        "pool_share trustlines belong to extract_lp_positions, not asset rows"
    );
}

#[test]
fn classic_credit_skips_non_trustline_entries() {
    let change = make_change(
        "account",
        "created",
        json!({"account_id": "GACCOUNT"}),
        Some(json!({"account_id": "GACCOUNT", "balance": 10_000_000})),
    );
    let assets = detect_classic_credit_assets(&[change]);
    assert!(assets.is_empty());
}

#[test]
fn classic_credit_skips_malformed_asset_object() {
    // Asset object missing `code` or `issuer` should be skipped rather
    // than producing a partial-identity row.
    let no_code = make_change(
        "trustline",
        "created",
        json!({"account_id": "GHOLDER", "asset": {"type": "credit_alphanum4"}}),
        Some(json!({
            "account_id": "GHOLDER",
            "asset": { "type": "credit_alphanum4", "issuer": "GISSUER" },
            "balance": 0,
        })),
    );
    let no_issuer = make_change(
        "trustline",
        "created",
        json!({"account_id": "GHOLDER", "asset": {"type": "credit_alphanum4"}}),
        Some(json!({
            "account_id": "GHOLDER",
            "asset": { "type": "credit_alphanum4", "code": "USDC" },
            "balance": 0,
        })),
    );
    let assets = detect_classic_credit_assets(&[no_code, no_issuer]);
    assert!(assets.is_empty());
}

#[test]
fn native_singleton_returns_native_asset_no_identity() {
    let asset = native_asset_singleton();
    assert_eq!(asset.asset_type, AssetFamily::Native);
    assert!(asset.asset_code.is_none());
    assert!(asset.issuer_address.is_none());
    assert!(asset.contract_id.is_none());
}
