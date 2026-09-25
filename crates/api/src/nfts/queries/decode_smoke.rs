//! Live-CH decode smoke for the NFT read path. The curl `FORMAT` box smokes
//! do NOT exercise the clickhouse-rs RowBinary decoder, so a wire-type↔struct
//! mismatch (e.g. a Nullable column decoded into a non-Option field, or a
//! positional reorder) passes a curl check yet 500s the live endpoint. This
//! decodes rows a real CH produced for each NFT fetch fn.
//!
//! **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green.
//! Run against a reachable CH (local replica or SSH tunnel):
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib nfts::queries::decode_smoke -- --nocapture
//! ```

use super::*;
use crate::common::cursor::Direction;

fn client() -> Option<clickhouse::Client> {
    let url = std::env::var("CH_URL").ok()?;
    let mut c = clickhouse::Client::default().with_url(url);
    if let Ok(u) = std::env::var("CH_USER") {
        c = c.with_user(u);
    }
    if let Ok(p) = std::env::var("CH_PASSWORD") {
        c = c.with_password(p);
    }
    if let Ok(d) = std::env::var("CH_DATABASE") {
        c = c.with_database(d);
    }
    Some(c)
}

/// Every NFT CH row struct must decode the rows a real CH emits.
#[tokio::test]
async fn nft_ch_rows_decode() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping NFT CH decode smoke");
        return;
    };

    // `list` returns rows on any populated CH → always exercises the
    // `NftListChRow` decode, and bootstraps a real (contract_id, token_id)
    // for the per-NFT fetches below.
    let params = ResolvedListParams {
        limit: 5,
        cursor: None,
        filter_collection: None,
        filter_contract_id: None,
        filter_name: None,
    };
    let list = fetch_list(&ch, &params, Direction::Next)
        .await
        .expect("NftListChRow must decode");

    let Some(first) = list.first() else {
        eprintln!("CH has no NFTs — list decode ok, skipping per-NFT smoke");
        return;
    };
    let (contract_id, token_id) = (first.contract_id.clone(), first.token_id.clone());

    nft_exists(&ch, &contract_id, &token_id)
        .await
        .expect("nft_exists must run");
    fetch_by_composite(&ch, &contract_id, &token_id)
        .await
        .expect("NftChRow (detail) must decode");
    fetch_transfers(&ch, &contract_id, &token_id, None, 5, Direction::Next)
        .await
        .expect("NftTransferChRow must decode");
}

/// Task 0528 regression — a token whose stored `nfts.minted_at_ledger` was
/// clobbered to NULL by a later transfer / burn must still SERVE its mint
/// ledger, derived from the append-only `nft_ownership`.
///
/// Fails on the pre-0528 code, which read the stored column and served
/// `None` for every such token (621 / 13 915 on prod when this was filed).
///
/// Picks its own subject: any token that is clobbered AND has a Mint row.
/// Skips cleanly when the CH under test has none — a freshly seeded CH
/// where no burn has landed yet is a legitimate empty case, not a failure.
#[tokio::test]
async fn clobbered_mint_ledger_is_served_from_ownership() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping 0528 mint-ledger regression");
        return;
    };

    // One clobbered token + the mint ledger the journal still holds for it.
    let subject = ch
        .query(
            "SELECT sc.contract_id, n.token_id, m.minted_at_ledger \
             FROM ( \
                 SELECT contract_id, token_id \
                 FROM nfts \
                 GROUP BY contract_id, token_id \
                 HAVING argMax(minted_at_ledger, current_owner_ledger) IS NULL \
             ) n \
             INNER JOIN ( \
                 SELECT contract_id, token_id, min(ledger_sequence) AS minted_at_ledger \
                 FROM nft_ownership \
                 WHERE event_type = 0 \
                 GROUP BY contract_id, token_id \
             ) m ON m.contract_id = n.contract_id AND m.token_id = n.token_id \
             INNER JOIN soroban_contracts sc ON sc.id = n.contract_id \
             LIMIT 1",
        )
        .fetch_optional::<(String, String, i64)>()
        .await
        .expect("subject probe must run");

    let Some((contract_id, token_id, expected)) = subject else {
        eprintln!("no clobbered NFT on this CH — skipping 0528 regression");
        return;
    };

    let item = fetch_by_composite(&ch, &contract_id, &token_id)
        .await
        .expect("detail must decode")
        .expect("subject token must exist in nfts");

    assert_eq!(
        item.minted_at_ledger,
        Some(expected),
        "detail served the clobbered stored column instead of deriving the \
         mint ledger from nft_ownership (contract {contract_id}, token {token_id})"
    );
}

/// Task 0528 — keyset pagination must stay TOTAL now that the lead sort key
/// is derived rather than stored.
///
/// The risk this covers: the ORDER BY, the keyset predicate and the cursor
/// payload each reference the mint ledger separately. If any one of them
/// still read `nfts.minted_at_ledger` while the others read the derived
/// value, pages would order by one key and seek by another — silently
/// skipping or repeating rows, which no single-page test would notice.
/// Clobbered and healthy tokens interleave by mint ledger, so a mismatch
/// cannot cancel out.
///
/// Walks the whole list in 2-row pages and asserts every token is seen
/// exactly once, in non-increasing mint-ledger order.
#[tokio::test]
async fn keyset_pagination_is_total_over_derived_mint_ledger() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping 0528 pagination totality check");
        return;
    };

    let total = ch
        .query("SELECT count() FROM (SELECT contract_id, token_id FROM nfts GROUP BY contract_id, token_id)")
        .fetch_one::<u64>()
        .await
        .expect("count probe must run") as usize;
    if total < 2 {
        eprintln!("CH has <2 NFTs — skipping 0528 pagination totality check");
        return;
    }

    const PAGE: usize = 2;
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut prev_ledger: Option<i64> = None;
    let mut cursor = None;

    // `total` pages of `PAGE` rows is a strict upper bound; overrunning it
    // means the cursor stopped advancing (repeat loop), which is itself the
    // failure we are hunting.
    for _ in 0..=total {
        let params = ResolvedListParams {
            // `limit` is the handler's peek+1, so PAGE rows come back plus
            // one lookahead we drop.
            limit: (PAGE + 1) as i64,
            cursor: cursor.take(),
            filter_collection: None,
            filter_contract_id: None,
            filter_name: None,
        };
        let mut rows = fetch_list(&ch, &params, Direction::Next)
            .await
            .expect("page must decode");
        let has_more = rows.len() > PAGE;
        rows.truncate(PAGE);
        if rows.is_empty() {
            break;
        }

        for r in &rows {
            let ledger = r.minted_at_ledger.unwrap_or(0);
            if let Some(p) = prev_ledger {
                assert!(
                    ledger <= p,
                    "mint-ledger order broke across the page boundary: {ledger} after {p} \
                     — ORDER BY and the keyset predicate disagree"
                );
            }
            prev_ledger = Some(ledger);
            seen.push((r.contract_id.clone(), r.token_id.clone()));
        }

        if !has_more {
            break;
        }
        let last = rows.last().expect("non-empty");
        cursor = Some(NftListCursor {
            minted_at_ledger: last.minted_at_ledger.unwrap_or(0),
            contract_surrogate: last.contract_surrogate,
            token_id: last.token_id.clone(),
        });
    }

    let mut deduped = seen.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        seen.len(),
        "pagination repeated a token — cursor and sort key disagree"
    );
    assert_eq!(
        seen.len(),
        total,
        "pagination skipped tokens: walked {} of {total}",
        seen.len()
    );
}
