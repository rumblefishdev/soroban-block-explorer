use std::collections::HashSet;

use super::*;
use crate::common::ch::test_client_from_env;

#[derive(Debug, Row, Deserialize)]
struct BusiestToken {
    share_token_id: i64,
    holders: u64,
}

/// Walking every page of the busiest share token's holders must reach each
/// holder exactly once, in descending order, and drop none.
///
/// Invisible to a unit test — the keyset is inline SQL over a window, and one
/// page alone always looks right. Needs a ClickHouse holding pool data (CH_URL);
/// skips otherwise. A holder the dimensions cannot resolve is dropped by
/// design and fails the count here, which is the point: the list and the
/// participant count must agree.
#[tokio::test]
async fn paging_reaches_every_holder_once() {
    let Some(ch) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban participants paging smoke");
        return;
    };
    let token = ch
        .query(
            "SELECT asset_id AS share_token_id, count() AS holders FROM ( \
                 SELECT asset_id, holder_id FROM balances \
                 WHERE asset_id IN (SELECT share_token_id FROM pool_instance_state FINAL \
                                    WHERE share_token_id != 0) \
                 GROUP BY asset_id, holder_id \
                 HAVING argMax(amount, last_updated_ledger) > 0) \
             GROUP BY asset_id ORDER BY holders DESC LIMIT 1",
        )
        .fetch_optional::<BusiestToken>()
        .await
        .expect("busiest share token query");
    let Some(token) = token else {
        eprintln!("no share-token holders on this ClickHouse — skipping");
        return;
    };

    const PAGE: i64 = 50;
    let mut cursor: Option<SharesCursor> = None;
    let mut seen = HashSet::new();
    let mut last_amount: Option<i128> = None;
    loop {
        let page = fetch_soroban_participants(
            &ch,
            token.share_token_id,
            0,
            cursor.as_ref(),
            PAGE,
            Direction::Next,
        )
        .await
        .expect("holder page decodes");
        for row in &page {
            assert!(
                seen.insert(row.account_id_surrogate),
                "holder {} appears on two pages",
                row.account
            );
            let amount: i128 = row.cursor_shares.parse().expect("raw integer balance");
            assert!(
                last_amount.is_none_or(|prev| amount <= prev),
                "pages do not descend"
            );
            last_amount = Some(amount);
        }
        let Some(last) = page.last() else { break };
        cursor = Some(SharesCursor {
            shares: last.cursor_shares.clone(),
            account_id: last.account_id_surrogate,
        });
        if (page.len() as i64) < PAGE {
            break;
        }
    }
    assert_eq!(
        seen.len() as u64,
        token.holders,
        "listed holders != holders"
    );
}
