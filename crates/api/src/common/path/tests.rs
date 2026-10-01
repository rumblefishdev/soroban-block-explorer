use super::*;
use axum::body;
use axum::http::StatusCode;

async fn body_json(resp: Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

// -----------------------------------------------------------------------
// hash
// -----------------------------------------------------------------------

#[test]
fn hash_valid_lowercase_accepted() {
    let h = "ab".repeat(32); // 64 chars, all hex
    assert_eq!(parse_hash(&h).unwrap(), h);
}

#[test]
fn hash_valid_uppercase_normalised_to_lowercase() {
    let h = "AB".repeat(32);
    assert_eq!(parse_hash(&h).unwrap(), "ab".repeat(32));
}

#[test]
fn hash_valid_mixed_case_normalised_to_lowercase() {
    let h = "aB".repeat(32);
    assert_eq!(parse_hash(&h).unwrap(), "ab".repeat(32));
}

#[tokio::test]
async fn hash_wrong_length_rejected_with_invalid_hash() {
    let err = parse_hash("abcdef").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_hash");
    assert_eq!(json["details"]["param"], "hash");
    assert_eq!(json["details"]["received"], "abcdef");
}

#[tokio::test]
async fn hash_non_hex_char_rejected() {
    let mut h = "ab".repeat(31); // 62 chars
    h.push_str("XX"); // 64 total, X not hex
    let err = parse_hash(&h).unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_hash");
}

#[tokio::test]
async fn hash_empty_rejected() {
    let err = parse_hash("").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_hash");
}

// -----------------------------------------------------------------------
// strkey
// -----------------------------------------------------------------------

// Synthetic shape-valid StrKeys (no CRC): prefix + 55 base32 chars = 56.
const VALID_C: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";
const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";

#[test]
fn strkey_contract_accepted() {
    assert!(strkey(VALID_C, 'C', "contract_id").is_ok());
}

#[test]
fn strkey_account_accepted() {
    assert!(strkey(VALID_G, 'G', "account_id").is_ok());
}

#[tokio::test]
async fn strkey_contract_wrong_prefix_uses_invalid_contract_id() {
    // Wrong prefix `G` against helper expecting `C` → contract code
    let err = strkey(VALID_G, 'C', "contract_id").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
    assert_eq!(json["details"]["param"], "contract_id");
    assert_eq!(json["details"]["expected_prefix"], "C");
}

#[tokio::test]
async fn strkey_account_wrong_prefix_uses_invalid_account_id() {
    let err = strkey(VALID_C, 'G', "account_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_account_id");
    assert_eq!(json["details"]["expected_prefix"], "G");
}

#[tokio::test]
async fn strkey_wrong_length_rejected() {
    let err = strkey("CTOO_SHORT", 'C', "contract_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_contract_id");
}

#[tokio::test]
async fn strkey_invalid_alphabet_rejected() {
    // Keep the correct 56-character shape and `C` prefix, but inject `0`
    // (not in the RFC 4648 base32 alphabet) so the failure is unambiguously
    // an alphabet violation rather than a length mismatch.
    let mut bad = VALID_C.to_string();
    bad.replace_range(1..2, "0");
    assert_eq!(bad.len(), 56);
    let err = strkey(&bad, 'C', "contract_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_contract_id");
}

// -----------------------------------------------------------------------
// pool_id_strkey
// -----------------------------------------------------------------------

fn zero_pool_strkey() -> String {
    // Round-trippable LP strkey for the 32-byte all-zeroes payload.
    // Use the canonical encoder so the test stays valid if SEP-23
    // changes (it shouldn't, but coupling the test to the crate
    // avoids hand-computing CRC16-XModem).
    stellar_strkey::LiquidityPool([0u8; 32])
        .to_string()
        .to_string()
}

#[test]
fn pool_id_strkey_valid_accepted_and_decoded_to_lowercase_hex() {
    let strkey = zero_pool_strkey();
    let hex = pool_id_strkey(&strkey, "pool_id").unwrap();
    assert_eq!(hex.len(), 64);
    assert_eq!(hex, "0".repeat(64));
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    );
}

/// A Soroban pool IS a contract, so its identifier is the `C…` address.
/// This route accepted `L…` only, which 404'd every Soroban pool the list
/// itself linked to — found by running the UI against production data.
#[test]
fn pool_id_accepts_the_contract_form_a_soroban_pool_uses() {
    let c = "CB5D4HH5S6HZKJKANFAE5QZSJQLEQ65J26TFH42D2ZTS33XZVC7DBDBN";
    let hex = pool_id_strkey(c, "pool_id").expect("a soroban pool id must parse");
    assert_eq!(hex.len(), 64);
    // Same 32 bytes either way — only the version byte differs.
    let l = crate::common::strkey::pool_id_hex_to_strkey(&hex, domain::PoolKind::Classic);
    assert_eq!(
        pool_id_strkey(&l, "pool_id").expect("the classic form still parses"),
        hex
    );
}

#[tokio::test]
async fn pool_id_strkey_hex_rejected_with_strkey_hint() {
    // Hex form was the legacy wire shape; rejected post-0264 with an
    // informative envelope pointing the client at the strkey form.
    let hex = "0".repeat(64);
    let err = pool_id_strkey(&hex, "pool_id").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_pool_id");
    assert_eq!(json["details"]["param"], "pool_id");
    assert_eq!(json["details"]["expected_prefix"], "L or C");
}

#[tokio::test]
async fn pool_id_strkey_wrong_prefix_rejected() {
    // 56-char shape-valid strkey but C-prefix (contract) — must not
    // sneak through the pool validator.
    let bad = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";
    let err = pool_id_strkey(bad, "pool_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn pool_id_strkey_bad_crc_rejected() {
    // Take a valid LP strkey and flip the trailing checksum char.
    // Shape passes but CRC fails — pool_id_strkey is CRC-strict
    // (unlike strkey() for accounts/contracts) because the internal
    // DB form is the payload hash, not the strkey itself.
    let mut bad = zero_pool_strkey();
    let last = bad.pop().unwrap();
    bad.push(if last == 'A' { 'B' } else { 'A' });
    assert_eq!(bad.len(), 56);
    let err = pool_id_strkey(&bad, "pool_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn pool_id_strkey_wrong_length_rejected() {
    let err = pool_id_strkey("L", "pool_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn pool_id_strkey_empty_rejected() {
    let err = pool_id_strkey("", "pool_id").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_pool_id");
}

// -----------------------------------------------------------------------
// sequence
// -----------------------------------------------------------------------

#[test]
fn sequence_valid_accepted() {
    assert_eq!(sequence("1").unwrap(), 1);
    assert_eq!(sequence("12345678").unwrap(), 12_345_678);
    assert_eq!(sequence("4294967295").unwrap(), u32::MAX);
}

#[tokio::test]
async fn sequence_zero_rejected() {
    // Stellar ledger 0 does not exist — genesis is sequence 1.
    let err = sequence("0").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_sequence");
    assert_eq!(json["details"]["received"], "0");
}

#[tokio::test]
async fn sequence_negative_rejected() {
    let err = sequence("-1").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_sequence");
    assert_eq!(json["details"]["received"], "-1");
}

#[tokio::test]
async fn sequence_overflow_rejected() {
    // u32::MAX + 1 = 4294967296 — overflows.
    let err = sequence("4294967296").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_sequence");
}

#[tokio::test]
async fn sequence_non_numeric_rejected() {
    let err = sequence("abc").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_sequence");
}
