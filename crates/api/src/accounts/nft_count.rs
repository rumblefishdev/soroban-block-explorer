//! How many NFTs an account holds now, for the account detail (task 0675).
//!
//! The account page shows NFTs as a tab of its Assets card, with this count on
//! the tab; knowing it with the account lets the page lay the card out once,
//! and lets an account with none skip the NFT request altogether.

#[cfg(test)]
mod tests;

/// The holding is the `FINAL` row of `nfts`: a token sold in a later ingest
/// batch can sit unmerged beside its old row, and must not count — the same
/// rule as the owner read behind `GET /v1/nfts?filter[owner]=`.
pub async fn fetch_nft_count(
    client: &clickhouse::Client,
    account_surrogate_id: i64,
) -> Result<u64, clickhouse::error::Error> {
    client
        .query("SELECT count() FROM nfts FINAL WHERE current_owner_id = ?")
        .bind(account_surrogate_id)
        .fetch_one::<u64>()
        .await
}
