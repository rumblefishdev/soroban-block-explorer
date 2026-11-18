use serde_json::json;

use super::scval_integers_as_text;

fn converted(mut v: serde_json::Value) -> serde_json::Value {
    scval_integers_as_text(&mut v);
    v
}

#[test]
fn a_u64_above_the_browser_limit_keeps_every_digit() {
    // A real muxed id from a USDC transfer (tx e376edec…a19a): a browser's
    // JSON.parse reads the number as …588000.
    assert_eq!(
        converted(json!({"type": "u64", "value": 6952519163801588163u64})),
        json!({"type": "u64", "value": "6952519163801588163"})
    );
}

#[test]
fn every_integer_type_leaves_as_decimal_text_small_ones_too() {
    let cases = [
        (
            json!({"type": "u32", "value": 5}),
            json!({"type": "u32", "value": "5"}),
        ),
        (
            json!({"type": "i32", "value": -7}),
            json!({"type": "i32", "value": "-7"}),
        ),
        (
            json!({"type": "i64", "value": -9223372036854775808i64}),
            json!({"type": "i64", "value": "-9223372036854775808"}),
        ),
        (
            json!({"type": "timepoint", "value": 1795006369u64}),
            json!({"type": "timepoint", "value": "1795006369"}),
        ),
        (
            json!({"type": "duration", "value": 60}),
            json!({"type": "duration", "value": "60"}),
        ),
        (
            json!({"type": "ledger_key_nonce", "value": 8070450532247928832i64}),
            json!({"type": "ledger_key_nonce", "value": "8070450532247928832"}),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(converted(input), expected);
    }
}

#[test]
fn u256_and_i256_leave_as_signed_decimal_not_hex() {
    // The parser writes the four 64-bit parts as 64 hex characters; the
    // official stellar-xdr JSON (SEP-51) writes the decimal value.
    assert_eq!(
        converted(json!({
            "type": "u256",
            "value": "0000000000000000000000000000000000000000000000000000000000000010"
        })),
        json!({"type": "u256", "value": "16"})
    );
    assert_eq!(
        converted(json!({
            "type": "i256",
            "value": "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb"
        })),
        json!({"type": "i256", "value": "-5"})
    );
    // Real value from a mainnet event (ledger 65540000+).
    assert_eq!(
        converted(json!({
            "type": "u256",
            "value": "0000000000000000000000000000000000000002a58361f9c73d393e1de1ba0f"
        })),
        json!({"type": "u256", "value": "209680183608564934783906462223"})
    );
}

#[test]
fn values_already_text_and_non_integers_stay_as_they_are() {
    let same = [
        json!({"type": "i128", "value": "-170141183460469231731687303715884105728"}),
        json!({"type": "u128", "value": "340282366920938463463374607431768211455"}),
        json!({"type": "sym", "value": "transfer"}),
        json!({"type": "bool", "value": true}),
        json!({"type": "void", "value": null}),
        json!({"type": "address", "value": "GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ"}),
    ];
    for v in same {
        assert_eq!(converted(v.clone()), v);
    }
}

#[test]
fn integers_nested_in_vec_map_and_instance_storage_are_converted() {
    let input = json!({
        "type": "vec",
        "value": [
            {"type": "map", "value": [
                {"key": {"type": "sym", "value": "amount_out"},
                 "value": {"type": "u64", "value": 9007199254740993u64}}
            ]},
            {"type": "contract_instance", "value": {
                "executable": {"type": "wasm", "hash": "ab"},
                "storage": [
                    {"key": {"type": "u32", "value": 1},
                     "value": {"type": "i64", "value": 42}}
                ]
            }}
        ]
    });
    let expected = json!({
        "type": "vec",
        "value": [
            {"type": "map", "value": [
                {"key": {"type": "sym", "value": "amount_out"},
                 "value": {"type": "u64", "value": "9007199254740993"}}
            ]},
            {"type": "contract_instance", "value": {
                "executable": {"type": "wasm", "hash": "ab"},
                "storage": [
                    {"key": {"type": "u32", "value": "1"},
                     "value": {"type": "i64", "value": "42"}}
                ]
            }}
        ]
    });
    assert_eq!(converted(input), expected);
}

#[test]
fn untyped_numbers_around_contract_values_are_left_alone() {
    // Classic operation details carry plain numbers; only typed contract
    // values are in scope (lore 0363, decision 13 step 1).
    let input = json!({
        "amount": 10000000,
        "functionArgs": [{"type": "u32", "value": 3}],
    });
    let expected = json!({
        "amount": 10000000,
        "functionArgs": [{"type": "u32", "value": "3"}],
    });
    assert_eq!(converted(input), expected);
}
