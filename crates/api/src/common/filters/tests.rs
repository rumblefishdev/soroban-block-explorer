use super::*;
use axum::body;
use axum::http::StatusCode;

async fn body_json(resp: Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

// Synthetic shape-valid StrKeys: prefix char + 55 body chars = 56 total,
// all in RFC 4648 base32 alphabet. Not real addresses; CRC is not
// validated by `strkey()` on purpose.
const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";
const VALID_C: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";

// Length-check sanity: the StrKey prefix + 55 more chars = 56. The
// constants above are synthetic shape-valid placeholders, not real
// addresses — length is asserted explicitly because getting this
// wrong in a test constant would mask validator bugs.
#[test]
fn test_constants_are_56_chars() {
    assert_eq!(VALID_G.len(), 56);
    assert_eq!(VALID_C.len(), 56);
}

#[test]
fn strkey_valid_account_accepted() {
    assert!(strkey(VALID_G, 'G', "source_account").is_ok());
}

#[test]
fn strkey_valid_contract_accepted() {
    assert!(strkey(VALID_C, 'C', "contract_id").is_ok());
}

#[tokio::test]
async fn strkey_wrong_prefix_rejected() {
    let err = strkey(VALID_C, 'G', "source_account").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "source_account");
}

#[tokio::test]
async fn strkey_wrong_length_rejected() {
    let err = strkey("GTOO_SHORT", 'G', "source_account").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
}

#[tokio::test]
async fn strkey_invalid_alphabet_rejected() {
    // Contains `0` and `1`, which are not in RFC 4648 base32.
    let bad = "G0000000000000000000000000000000000000000000000000001T";
    let err = strkey(bad, 'G', "source_account").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
}

// A tiny standalone enum so we do not couple the unit test to
// `domain::OperationType`. The real-world use site is still
// exercised via the retro-refactored transactions handler in Step 8.
#[derive(Debug, PartialEq)]
enum Kind {
    Alpha,
    Beta,
}
impl FromStr for Kind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ALPHA" => Ok(Kind::Alpha),
            "BETA" => Ok(Kind::Beta),
            _ => Err(()),
        }
    }
}

#[test]
fn parse_enum_accepts_known_variant() {
    assert_eq!(
        parse_enum::<Kind>("ALPHA", "kind", None).unwrap(),
        Kind::Alpha
    );
}

#[tokio::test]
async fn parse_enum_rejects_unknown_variant_with_generic_hint() {
    let err = parse_enum::<Kind>("GAMMA", "kind", None).unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["received"], "GAMMA");
    assert_eq!(json["message"], "filter[kind] is not a recognized value");
}

#[tokio::test]
async fn parse_enum_rejects_unknown_variant_with_kind_hint() {
    let err = parse_enum::<Kind>("GAMMA", "kind", Some("kind name")).unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(
        json["message"],
        "filter[kind] is not a recognized kind name"
    );
}

// -----------------------------------------------------------------------
// reject_sql_wildcards
// -----------------------------------------------------------------------

#[test]
fn wildcards_clean_value_accepted() {
    assert!(reject_sql_wildcards("punk", "name").is_ok());
    assert!(reject_sql_wildcards("Stellar Punks", "collection").is_ok());
    // Empty also OK — caller's Option-handling decides whether absent
    // is allowed; the helper itself only checks character content.
    assert!(reject_sql_wildcards("", "name").is_ok());
}

#[tokio::test]
async fn wildcards_percent_rejected() {
    let err = reject_sql_wildcards("100%real", "name").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "name");
    assert_eq!(json["details"]["received"], "100%real");
}

#[tokio::test]
async fn wildcards_underscore_rejected() {
    let err = reject_sql_wildcards("foo_bar", "code").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "code");
}

#[test]
fn wildcards_opt_none_passes() {
    assert!(reject_sql_wildcards_opt(None, "name").is_ok());
}

#[tokio::test]
async fn wildcards_opt_some_validates() {
    assert!(reject_sql_wildcards_opt(Some("punk"), "name").is_ok());
    let err = reject_sql_wildcards_opt(Some("%"), "name").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
}

// -----------------------------------------------------------------------
// parse_iso8601
// -----------------------------------------------------------------------

#[test]
fn iso8601_z_suffix_accepted() {
    let parsed = parse_iso8601("2026-04-30T12:00:00Z", "from").unwrap();
    assert_eq!(parsed.to_rfc3339(), "2026-04-30T12:00:00+00:00");
}

#[test]
fn iso8601_offset_accepted_and_normalised_to_utc() {
    let parsed = parse_iso8601("2026-04-30T14:00:00+02:00", "from").unwrap();
    // +02:00 → 14:00 local = 12:00 UTC
    assert_eq!(parsed.to_rfc3339(), "2026-04-30T12:00:00+00:00");
}

#[tokio::test]
async fn iso8601_naive_without_timezone_rejected() {
    // Naive timestamp (no Z, no offset) — explorer API stays in UTC.
    let err = parse_iso8601("2026-04-30T12:00:00", "from").unwrap_err();
    let (status, json) = body_json(err).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "from");
}

#[tokio::test]
async fn iso8601_garbage_rejected() {
    let err = parse_iso8601("not-a-timestamp", "to").unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "to");
}

const REAL_C: &str = "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA";
/// `REAL_C` with sub-account id 42 (CAP-0084).
const REAL_W: &str = "WDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIAAAAAAAAAAAFLR56";

#[tokio::test]
async fn contract_id_opt_accepts_c_and_w_and_rejects_the_rest() {
    assert_eq!(contract_id_opt(None).unwrap(), None);
    assert_eq!(
        contract_id_opt(Some(REAL_C)).unwrap().as_deref(),
        Some(REAL_C)
    );
    assert_eq!(
        contract_id_opt(Some(REAL_W)).unwrap().as_deref(),
        Some(REAL_C)
    );
    let err = contract_id_opt(Some(VALID_G)).unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "contract_id");
}

const REAL_G: &str = "GD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2FTL";
/// `REAL_G` with sub-account id 3539365402.
const REAL_M: &str = "MD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2AAAAAANF5TODJEYA";

#[tokio::test]
async fn account_id_opt_accepts_g_and_m_and_rejects_the_rest() {
    assert_eq!(account_id_opt(None).unwrap(), None);
    assert_eq!(
        account_id_opt(Some(REAL_G)).unwrap().as_deref(),
        Some(REAL_G)
    );
    assert_eq!(
        account_id_opt(Some(REAL_M)).unwrap().as_deref(),
        Some(REAL_G)
    );
    let err = account_id_opt(Some(REAL_C)).unwrap_err();
    let (_, json) = body_json(err).await;
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "source_account");
}
