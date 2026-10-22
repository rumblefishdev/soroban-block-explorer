use super::*;

/// `token_uri(0)` of the Stroopies collection (CDC5ZX…JFGB) on mainnet,
/// 2026-10-21: plain JSON, its `image` an inline SVG.
const STROOPIES_TOKEN_0: &str = include_str!("stroopies_token_0.txt");

/// `token_uri(4)` of the SushiSwap V3 positions contract (CARTUL…UQZ4) on
/// mainnet, 2026-10-22: base64 JSON with a name and no image.
const SUSHISWAP_TOKEN_4: &str = "data:application/json;base64,eyJuYW1lIjoiU3VzaGlzd2FwIC0gMC4zJSAtIFNvbHZCVEMvVVNEQyAtIE1JTjw+TUFYIiwgImRlc2NyaXB0aW9uIjoiVGhpcyBORlQgcmVwcmVzZW50cyBhIGxpcXVpZGl0eSBwb3NpdGlvbiBpbiBhIFN1c2hpc3dhcCBWMyBTb2x2QlRDLVVTREMgcG9vbC4gVGhlIG93bmVyIG9mIHRoaXMgTkZUIGNhbiBtb2RpZnkgb3IgcmVkZWVtIHRoZSBwb3NpdGlvbi5cblxuUG9vbCBBZGRyZXNzOiBDQkJYWkROTklWQ0dHTExDSDQzS1RINldEWjVNRlZZRlE1TFRHRDIzNkE0QTZPSk1IMks0SDZMQVxuU29sdkJUQyBBZGRyZXNzOiBDQklKQkROWk5GNFgzNUJKNEZGWldDREJTQ0tPUDVOQjRQTEc0U05FTlJNTEFQWUc0UDVGTTZWTlxuVVNEQyBBZGRyZXNzOiBDQ1c2N1RTWlYzU1NTMkhYTUJRNUpGR0NLSk5YS1pNN1VRVVdVWlBVVEhYU1RaTEVPN1NKTUk3NVxuRmVlIFRpZXI6IDAuMyVcblRva2VuIElEOiA0XG5cbuKaoO+4jyBESVNDTEFJTUVSOiBEdWUgZGlsaWdlbmNlIGlzIGltcGVyYXRpdmUgd2hlbiBhc3Nlc3NpbmcgdGhpcyBORlQuIE1ha2Ugc3VyZSB0b2tlbiBhZGRyZXNzZXMgbWF0Y2ggdGhlIGV4cGVjdGVkIHRva2VucywgYXMgdG9rZW4gc3ltYm9scyBtYXkgYmUgaW1pdGF0ZWQuIn0=";

#[test]
fn plain_json_is_parsed_without_a_request() {
    let json = metadata_from_data_uri(STROOPIES_TOKEN_0).expect("plain data: JSON parses");
    assert_eq!(json["name"], "Stroopies 0");
    let image = json["image"].as_str().expect("image is a string");
    assert!(image.starts_with("data:image/svg+xml,<svg "), "{image:.40}");
}

#[test]
fn base64_json_is_parsed_the_same_way() {
    let json = metadata_from_data_uri(SUSHISWAP_TOKEN_4).expect("base64 data: JSON parses");
    assert_eq!(json["name"], "Sushiswap - 0.3% - SolvBTC/USDC - MIN<>MAX");
    assert!(json.get("image").is_none());
}

#[test]
fn inline_image_is_the_direct_image_convention() {
    let uri = "data:image/png;base64,iVBORw0KGgo=";
    let json = metadata_from_data_uri(uri).expect("data:image is accepted");
    assert_eq!(json, serde_json::json!({ "image": uri }));
}

#[test]
fn other_media_types_are_unsupported() {
    for uri in [
        "data:text/html,<script>alert(1)</script>",
        "data:text/plain,hello",
        "data:text/json,{}",
        "data:,no-media-type-means-text-plain",
    ] {
        let err = metadata_from_data_uri(uri).expect_err(uri);
        assert!(
            matches!(err, NftTokenUriError::UnsupportedContentType(_)),
            "{uri}: {err}"
        );
    }
}

#[test]
fn a_value_that_does_not_parse_is_permanent() {
    let no_comma = metadata_from_data_uri("data:application/json").expect_err("no comma");
    assert!(
        matches!(no_comma, NftTokenUriError::MalformedUri { .. }),
        "{no_comma}"
    );

    let bad_base64 =
        metadata_from_data_uri("data:application/json;base64,@@@").expect_err("bad base64");
    assert!(
        matches!(bad_base64, NftTokenUriError::MalformedUri { .. }),
        "{bad_base64}"
    );

    let bad_json = metadata_from_data_uri("data:application/json,{\"name\":").expect_err("json");
    assert!(
        matches!(bad_json, NftTokenUriError::MalformedJson(_)),
        "{bad_json}"
    );
}

#[test]
fn a_body_over_the_metadata_cap_is_too_large() {
    let uri = format!(
        "data:application/json,{{\"name\":\"{}\"}}",
        "a".repeat(MAX_BODY_BYTES)
    );
    let err = metadata_from_data_uri(&uri).expect_err("over the cap");
    assert!(
        matches!(err, NftTokenUriError::BodyTooLarge { .. }),
        "{err}"
    );
}

#[test]
fn an_inline_image_over_the_cap_is_too_large() {
    let uri = format!("data:image/png;base64,{}", "A".repeat(MAX_BODY_BYTES));
    let err = metadata_from_data_uri(&uri).expect_err("over the cap");
    assert!(
        matches!(err, NftTokenUriError::BodyTooLarge { .. }),
        "{err}"
    );
}
