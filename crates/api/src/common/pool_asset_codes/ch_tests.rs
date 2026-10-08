//! ClickHouse-backed check of the pool asset filter (task 0636): the real
//! `init.sql`, the real predicate, a throwaway database. A Soroban token
//! stores no code, so its symbol and name must find its pool. Gated on
//! `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_leg_matched_by_symbol

use super::pool_asset_filter;

const DB: &str = "api_test_0636_pool_asset_codes";
const TOKEN: &str = "CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN";
/// An XLM / SolvBTC pool (XLM is asset id 1 here) and a pool of two
/// classic assets that do not match.
const POOL: &str = "5555555555555555555555555555555555555555555555555555555555555555";
const OTHER: &str = "5454545454545454545454545454545454545454545454545454545454545454";

#[tokio::test]
async fn soroban_leg_matched_by_symbol() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping pool asset filter check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");

    let token = db_clickhouse::persist::ids::contract_id(TOKEN);
    for sql in [
        format!(
            "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, id) VALUES \
             (0, '', 0, 0, 1), (1, 'USDC', 7, 0, 2), (1, 'AQUA', 7, 0, 3), (3, '', 0, {token}, {token})"
        ),
        format!(
            "INSERT INTO soroban_contracts (id, contract_id, is_sac) VALUES ({token}, '{TOKEN}', false)"
        ),
        format!(
            "INSERT INTO soroban_contract_metadata (contract_id, name, symbol, decimals, version) VALUES \
             ('{TOKEN}', 'Solv BTC', 'SolvBTC', 8, 1)"
        ),
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POOL}'), 30, 1, 1, [1, {token}]), (unhex('{OTHER}'), 30, 1, 0, [2, 3])"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    // By symbol, by a piece of the name, and as one half of a pair.
    assert_eq!(matched(&ch, &["SOLVBTC"]).await, vec![POOL.to_uppercase()]);
    assert_eq!(matched(&ch, &["SOLV BTC"]).await, vec![POOL.to_uppercase()]);
    assert_eq!(
        matched(&ch, &["XLM", "SOLVBTC"]).await,
        vec![POOL.to_uppercase()]
    );
    // A classic code still matches as before.
    assert_eq!(matched(&ch, &["USDC"]).await, vec![OTHER.to_uppercase()]);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}

/// The pools the filter for `needles` selects, as upper-case hex ids.
async fn matched(ch: &clickhouse::Client, needles: &[&str]) -> Vec<String> {
    let needles: Vec<String> = needles.iter().map(|n| n.to_string()).collect();
    let filter = pool_asset_filter(ch, &needles)
        .await
        .expect("token ids")
        .expect("clause");
    let mut q = ch.query(&format!(
        "SELECT hex(pool_id) FROM liquidity_pools AS lp FINAL WHERE {} ORDER BY 1",
        filter.sql
    ));
    for b in filter.binds {
        q = q.bind(b);
    }
    for (name, ids) in &filter.params {
        q = q.param(name, ids);
    }
    q.fetch_all::<String>().await.expect("filter runs")
}
