use super::*;
use xdr_parser::ExtractedSorobanBalance;

#[test]
fn build_balance_rows_maps_holder_and_asset_surrogates() {
    let extracted = vec![ExtractedSorobanBalance {
        contract_id: "CTOKEN1".into(),
        holder: "GHOLDER1".into(),
        balance: 800_009_446_178_i128,
        ledger: 100,
        closed: false,
    }];
    // No SAC map → type-3 keying: asset_id == the token's contract surrogate.
    let rows = build_balance_rows(&extracted, &HashMap::new());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].holder_id, ids::address_id("GHOLDER1"));
    assert_eq!(rows[0].asset_id, ids::contract_id("CTOKEN1"));
    assert_eq!(rows[0].amount, 800_009_446_178_i128);
    assert_eq!(rows[0].last_updated_ledger, 100);
}

#[test]
fn build_balance_rows_rekeys_sac_via_map_but_not_type3() {
    // A SAC contract-held balance + a type-3 token balance. The map re-keys
    // only the SAC (its surrogate → classic id); the type-3 token is absent
    // and keeps its own surrogate.
    let classic_usdc = ids::asset_id(1, "USDC", 42, 0);
    let sac_classic = HashMap::from([(ids::contract_id("CSAC"), classic_usdc)]);
    let rows = build_balance_rows(
        &[
            ExtractedSorobanBalance {
                contract_id: "CSAC".into(),
                holder: "CPOOL".into(),
                balance: 100,
                ledger: 10,
                closed: false,
            },
            ExtractedSorobanBalance {
                contract_id: "CTOKEN3".into(),
                holder: "GHOLDER".into(),
                balance: 200,
                ledger: 10,
                closed: false,
            },
        ],
        &sac_classic,
    );

    assert_eq!(
        rows[0].asset_id, classic_usdc,
        "SAC contract-held → classic id"
    );
    assert_eq!(
        rows[1].asset_id,
        ids::contract_id("CTOKEN3"),
        "type-3 unchanged"
    );
}

// ------------------------------------------------------------------
// extract_event_signature — the three mainnet topic conventions plus
// the monitored fourth arm (task 0517). Shapes are verbatim from the
// production measurement of 2026-09-02.
// ------------------------------------------------------------------

#[test]
fn signature_from_a_symbol_first_topic() {
    let topics = serde_json::json!([
        {"type": "sym", "value": "transfer"},
        {"type": "address", "value": "GAAAA"}
    ]);
    assert_eq!(
        extract_event_signature(&topics).as_deref(),
        Some("transfer")
    );
}

#[test]
fn signature_from_the_label_convention() {
    // SoroswapPair / DeFindexVault / BlendStrategy: a String protocol
    // label first, the Symbol name second. The label is NOT lifted.
    let topics = serde_json::json!([
        {"type": "string", "value": "SoroswapPair"},
        {"type": "sym", "value": "sync"}
    ]);
    assert_eq!(extract_event_signature(&topics).as_deref(), Some("sync"));
}

#[test]
fn signature_from_the_phoenix_plain_str_convention() {
    // Phoenix publishes ("swap", "sender") as two Strings — the FIRST
    // is the name, the second discriminates the field.
    let topics = serde_json::json!([
        {"type": "string", "value": "swap"},
        {"type": "string", "value": "sender"}
    ]);
    assert_eq!(extract_event_signature(&topics).as_deref(), Some("swap"));
    // Single-String and String+bytes variants of the same family.
    let single = serde_json::json!([{"type": "string", "value": "Message"}]);
    assert_eq!(extract_event_signature(&single).as_deref(), Some("Message"));
    let with_bytes = serde_json::json!([
        {"type": "string", "value": "OrderCreated"},
        {"type": "bytes", "value": "AAAA"}
    ]);
    assert_eq!(
        extract_event_signature(&with_bytes).as_deref(),
        Some("OrderCreated")
    );
}

#[test]
fn an_unknown_convention_keeps_null_and_an_empty_vector_stays_silent() {
    // A hypothetical fourth convention (non-sym, non-string first
    // topic) resolves nowhere — NULL plus the monitor warn.
    let unknown = serde_json::json!([
        {"type": "u64", "value": "7"},
        {"type": "sym", "value": "name_here_is_not_taken"}
    ]);
    assert_eq!(extract_event_signature(&unknown), None);
    // No topics — nothing to resolve, silently.
    assert_eq!(extract_event_signature(&serde_json::json!([])), None);
    // Empty strings never become names.
    let empty = serde_json::json!([{"type": "string", "value": ""}]);
    assert_eq!(extract_event_signature(&empty), None);
}
