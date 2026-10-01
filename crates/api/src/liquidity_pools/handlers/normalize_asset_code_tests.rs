use super::normalize_asset_codes;

#[test]
fn none_passes_through() {
    assert!(normalize_asset_codes(None).is_empty());
}

#[test]
fn empty_string_becomes_none() {
    assert!(normalize_asset_codes(Some(String::new())).is_empty());
    assert!(normalize_asset_codes(Some("   ".into())).is_empty());
}

#[test]
fn lowercase_is_uppercased() {
    assert_eq!(normalize_asset_codes(Some("usdc".into())), ["USDC"]);
}

#[test]
fn mixed_case_is_uppercased() {
    assert_eq!(normalize_asset_codes(Some("UsDc".into())), ["USDC"]);
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    assert_eq!(normalize_asset_codes(Some("  xlm  ".into())), ["XLM"]);
}

#[test]
fn pair_splits_into_two_needles() {
    assert_eq!(
        normalize_asset_codes(Some("usdc/xlm".into())),
        ["USDC", "XLM"]
    );
}

#[test]
fn pair_tolerates_spaces_around_the_slash() {
    assert_eq!(
        normalize_asset_codes(Some(" usdc / xlm ".into())),
        ["USDC", "XLM"]
    );
}

#[test]
fn half_written_pair_keeps_the_written_half() {
    // Mid-typing state: the field debounces and fires on `USDC/`.
    assert_eq!(normalize_asset_codes(Some("USDC/".into())), ["USDC"]);
    assert_eq!(normalize_asset_codes(Some("/XLM".into())), ["XLM"]);
    assert!(normalize_asset_codes(Some("/".into())).is_empty());
}

#[test]
fn third_code_stays_inside_the_second_needle() {
    // `splitn(2)` bounds the needle count. The remainder is not discarded —
    // it becomes a needle no asset code can contain, so the query returns
    // nothing rather than silently answering a narrower question.
    assert_eq!(
        normalize_asset_codes(Some("USDC/XLM/BTC".into())),
        ["USDC", "XLM/BTC"]
    );
    assert!(normalize_asset_codes(Some("/".repeat(5_000))).len() <= 2);
}

#[test]
fn unicode_lower_uppercases_too() {
    // Stellar codes are ASCII-only in practice, but the normalizer
    // should not panic on UTF-8 — `String::to_uppercase` handles it.
    assert_eq!(normalize_asset_codes(Some("usdc🪙".into())), ["USDC🪙"]);
}
