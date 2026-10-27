use super::*;

#[test]
fn nft_with_name_or_media_is_real() {
    assert_eq!(nft_outcome("Stroopy #1", ""), EnrichOutcome::Real);
    assert_eq!(
        nft_outcome("", "https://arweave.net/x"),
        EnrichOutcome::Real
    );
    assert_eq!(
        nft_outcome("Stroopy #1", "https://arweave.net/x"),
        EnrichOutcome::Real
    );
}

#[test]
fn nft_with_neither_name_nor_media_is_sentinel() {
    // The collection name is stamped from the contract's `name()` whatever the
    // token returned, so it says nothing about this token's metadata.
    assert_eq!(nft_outcome("", ""), EnrichOutcome::Sentinel);
}
