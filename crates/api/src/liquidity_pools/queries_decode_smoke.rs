//! Live-CH **decode** smoke for the LP read path.
//!
//! The curl `FORMAT TSV/Vertical/JSON` box smokes do NOT exercise the
//! clickhouse-rs RowBinary decoder, so a wire-type↔struct mismatch — e.g. a
//! scalar `(SELECT count() …)` typed `Nullable(UInt64)` decoded into an `i64`
//! field (the detail `participant_count` bug, task 0243) — passes a curl check
//! yet 500s the live endpoint with `schema mismatch`. A pure-Rust round-trip
//! can't catch it either (the struct serializes consistently with itself). The
//! only real guard is decoding rows that an actual CH produced.
//!
//! This test runs each cheap LP CH fetch fn against a real CH and asserts the
//! rows decode (no error). It **skips cleanly when `CH_URL` is unset**, so CI
//! (no CH access) is unaffected. Run it against a reachable CH — a local
//! replica or an SSH tunnel to the box:
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib decode_smoke -- --nocapture
//! ```
//!
//! `transactions` is intentionally excluded: its driver scans the whole
//! `operations_appearances` table (~7.87B rows) until the `pool_id` projection
//! lands, so exercising it here would blow the read quota. Its row struct is all
//! direct, non-null columns (audited — no Nullable-decode risk).

use super::ResolvedPoolListParams;
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

/// `ChartChRow` reads money as `Nullable(Float64)` (task 0199 moved
/// formatting to Rust so chart and detail share one wire shape). That is
/// precisely the wire-type↔struct contract a pure-Rust test cannot check,
/// so assert it against a real server — including the NULL arm, which is
/// what an unpriced bucket returns.
///
/// Needs no schema, so any ClickHouse will do:
/// `docker run -d --rm -p 8123:8123 -e CLICKHOUSE_PASSWORD=probe clickhouse/clickhouse-server:26.3`
#[tokio::test]
async fn chart_row_decodes_nullable_floats() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping chart row decode smoke");
        return;
    };

    // The union is wrapped: ClickHouse resolves a top-level ORDER BY
    // against the union's own scope, where the branch aliases are not
    // visible (`Unknown expression identifier`).
    let rows = ch
        .query(
            "SELECT bucket_ms, tvl, volume, samples_in_bucket FROM ( \
                 SELECT toInt64(1700000000000)     AS bucket_ms, \
                        CAST(?, 'Nullable(Float64)')    AS tvl, \
                        CAST(?, 'Nullable(Float64)')    AS volume, \
                        toUInt64(7)                AS samples_in_bucket \
                 UNION ALL \
                 SELECT toInt64(1700000086400000)  AS bucket_ms, \
                        CAST(NULL, 'Nullable(Float64)') AS tvl, \
                        CAST(NULL, 'Nullable(Float64)') AS volume, \
                        toUInt64(0)                AS samples_in_bucket \
             ) ORDER BY bucket_ms",
        )
        .bind(25.31_f64)
        .bind(1.985_f64)
        .fetch_all::<ChartChRow>()
        .await
        .expect("ChartChRow decodes Nullable(Float64) from a real CH");

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].tvl, Some(25.31));
    assert_eq!(rows[0].volume, Some(1.985));
    assert_eq!(rows[0].samples_in_bucket, 7);
    // The unpriced bucket: NULL must survive as None, not decode as 0.0.
    assert_eq!(rows[1].tvl, None);
    assert_eq!(rows[1].volume, None);
}

/// Every LP CH row struct must decode the rows a real CH emits.
#[tokio::test]
async fn lp_ch_rows_decode() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping LP CH decode smoke");
        return;
    };

    // `list` returns rows on any populated CH → always exercises the
    // `PoolListChRow` decode, and bootstraps a guaranteed-real pool id for
    // the per-pool fetches below (an env-default pool might not exist on the
    // target CH → detail would return None and skip the decode entirely).
    let params = ResolvedPoolListParams {
        limit: 5,
        cursor: None,
        asset_a_code: None,
        asset_a_issuer: None,
        asset_b_code: None,
        asset_b_issuer: None,
        pool_id_hex: None,
        pool_kind: None,
        asset_codes: Vec::new(),
    };
    let pools = fetch_pool_list(&ch, &params, Direction::Next)
        .await
        .expect("list rows decode");

    let pool = match std::env::var("CH_TEST_POOL_HEX") {
        Ok(h) => h,
        Err(_) => match pools.first() {
            Some(r) => r.pool_id_hex.clone(),
            None => {
                eprintln!("CH has no liquidity pools — skipping per-pool decode");
                return;
            }
        },
    };

    // detail — `PoolDetailChRow`, incl. the Nullable-scalar `participant_count`.
    fetch_pool_by_id(&ch, &pool)
        .await
        .expect("detail row decodes");

    // participants — `ParticipantChRow`.
    fetch_participants(&ch, &pool, None, 5, Direction::Next)
        .await
        .expect("participant rows decode");

    // price context — `PriceContextChRow` (chart's 404 gate).
    let ctx = fetch_pool_price_context(&ch, &pool)
        .await
        .expect("price-context row decodes")
        .expect("bootstrapped pool exists");

    // The remaining two read `prices.*`, which the explorer does not own
    // and `schema/init.sql` does not create — a CH bootstrapped from this
    // repo alone has no such database. Probe once and skip rather than
    // fail, so the documented local-replica run still validates every
    // explorer-owned decode above. Against prod (or any CH with the
    // prices tenant) the probe passes and both are exercised — which also
    // proves the API user can read that database. No grant is needed
    // there: `api_reader` carries no `<grants>` block in
    // `users.d/services.xml` (unlike `prices_writer`/`prices_reader`,
    // where grants NARROW access), verified on the box 2026-08-04.
    if ch
        .query("SELECT 1 FROM prices.price_usd_series_1h LIMIT 1")
        .fetch_all::<u8>()
        .await
        .is_err()
    {
        eprintln!("`prices` database unreachable — skipping USD-analytics + chart decode");
        return;
    }

    // detail USD analytics — `Vol24ChRow` + `LastCloseChRow`.
    fetch_pool_usd_analytics(&ch, &pool, &ctx, None, None)
        .await
        .expect("usd-analytics rows decode");

    // chart — `ChartChRow`, incl. the `samples_in_bucket` UInt64.
    let to = chrono::Utc::now();
    let from = to - chrono::Duration::days(90);
    fetch_pool_chart(&ch, &pool, &ctx, "1d", from, to)
        .await
        .expect("chart rows decode");
}

/// `filter[asset_code]` is a substring of either leg, not an exact code
/// (0440 / issue #366). The regression this guards is the original
/// behaviour: `USD` returning nothing while the list is full of `USDC`
/// pools. Asserting the returned legs actually contain the needle also
/// catches the opposite failure — a predicate that stopped filtering.
#[tokio::test]
async fn asset_code_filter_matches_substring() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping LP asset-code substring smoke");
        return;
    };

    let params = ResolvedPoolListParams {
        limit: 10,
        cursor: None,
        asset_a_code: None,
        asset_a_issuer: None,
        asset_b_code: None,
        asset_b_issuer: None,
        // Deliberately a proper prefix of a real code: an exact-match
        // predicate returns zero rows here, a substring one does not.
        pool_id_hex: None,
        pool_kind: None,
        asset_codes: vec!["USD".to_string()],
    };
    let pools = fetch_pool_list(&ch, &params, Direction::Next)
        .await
        .expect("filtered list decodes");

    assert!(
        !pools.is_empty(),
        "`USD` matched no pool — substring filter regressed to exact match"
    );
    for p in &pools {
        let a = p.asset_a_code.as_deref().unwrap_or_default().to_uppercase();
        let b = p.asset_b_code.as_deref().unwrap_or_default().to_uppercase();
        assert!(
            a.contains("USD") || b.contains("USD"),
            "pool {} has neither leg containing USD ({a:?} / {b:?}) — filter not applied",
            p.pool_id_hex
        );
    }
}

/// `XLM` must reach the pools that hold *native* XLM. Native legs carry an
/// empty stored code, so a plain column match silently returns only the
/// credit assets minted under the code `XLM` — a wrong answer that looks
/// like a right one. Guards the `if(asset_type = 0, 'XLM', code)` alias.
#[tokio::test]
async fn asset_code_filter_finds_native_xlm() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping LP native-XLM smoke");
        return;
    };

    let params = ResolvedPoolListParams {
        limit: 25,
        cursor: None,
        asset_a_code: None,
        asset_a_issuer: None,
        asset_b_code: None,
        asset_b_issuer: None,
        pool_id_hex: None,
        pool_kind: None,
        asset_codes: vec!["XLM".to_string()],
    };
    let pools = fetch_pool_list(&ch, &params, Direction::Next)
        .await
        .expect("filtered list decodes");

    assert!(
        pools
            .iter()
            .any(|p| p.asset_a_type == 0 || p.asset_b_type == 0),
        "`XLM` returned {} pool(s) but none holds native XLM — the native \
         alias regressed and the filter is answering with look-alike \
         credit assets only",
        pools.len()
    );
}

/// A pair query constrains both legs and does not care which order the user
/// typed, nor which leg the chain assigned. Runs the same pair twice,
/// reversed, and requires identical results — the cheapest way to catch a
/// predicate that quietly became order-sensitive.
#[tokio::test]
async fn asset_code_filter_pair_is_order_insensitive() {
    let Some(ch) = client() else {
        eprintln!("CH_URL unset — skipping LP pair-filter smoke");
        return;
    };

    let pair = |a: &str, b: &str| ResolvedPoolListParams {
        limit: 25,
        cursor: None,
        asset_a_code: None,
        asset_a_issuer: None,
        asset_b_code: None,
        asset_b_issuer: None,
        pool_id_hex: None,
        pool_kind: None,
        asset_codes: vec![a.to_string(), b.to_string()],
    };

    let ids = |rows: Vec<PoolRow>| {
        let mut v: Vec<String> = rows.into_iter().map(|r| r.pool_id_hex).collect();
        v.sort();
        v
    };

    let forward = ids(fetch_pool_list(&ch, &pair("XLM", "USDC"), Direction::Next)
        .await
        .expect("forward pair decodes"));
    let reversed = ids(fetch_pool_list(&ch, &pair("USDC", "XLM"), Direction::Next)
        .await
        .expect("reversed pair decodes"));

    assert_eq!(forward, reversed, "pair filter is order-sensitive");
    assert!(
        !forward.is_empty(),
        "`XLM/USDC` matched no pool — the AND-ed needles are over-constraining"
    );

    // Both needles must bind: a pair that shares only one leg with any pool
    // has to come back empty, otherwise the second needle is being dropped.
    let impossible = fetch_pool_list(&ch, &pair("USDC", "ZZZZNOPE"), Direction::Next)
        .await
        .expect("impossible pair decodes");
    assert!(
        impossible.is_empty(),
        "pair with an unmatchable second needle returned {} pool(s) — the \
         needles are OR-ed, not AND-ed",
        impossible.len()
    );

    // Three codes. `normalize_asset_codes` splits `USDC/XLM/BTC` into
    // `USDC` and the literal `XLM/BTC` (see its unit tests); a pool has two
    // legs, so no asset code can carry that second needle and the answer is
    // empty. Asserted here so the query side cannot start "helpfully"
    // ignoring the remainder.
    let three = fetch_pool_list(&ch, &pair("USDC", "XLM/BTC"), Direction::Next)
        .await
        .expect("three-code query decodes");
    assert!(
        three.is_empty(),
        "a three-code query returned {} pool(s) — the third code is being \
         dropped instead of narrowing to nothing",
        three.len()
    );

    // Each needle claims its own leg. Repeating one therefore means "both
    // legs", not "matches somewhere, twice" — a pool with USDC on one side
    // and anything else on the other must not come back.
    let both_legs = fetch_pool_list(&ch, &pair("USDC", "USDC"), Direction::Next)
        .await
        .expect("repeated needle decodes");
    for p in &both_legs {
        let a = p.asset_a_code.as_deref().unwrap_or_default().to_uppercase();
        let b = p.asset_b_code.as_deref().unwrap_or_default().to_uppercase();
        assert!(
            a.contains("USDC") && b.contains("USDC"),
            "pool {} came back for `USDC/USDC` with legs {a:?} / {b:?} — one \
             asset is satisfying both needles",
            p.pool_id_hex
        );
    }
}
