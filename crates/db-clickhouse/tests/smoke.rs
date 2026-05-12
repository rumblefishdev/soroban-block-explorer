//! End-to-end smoke test for the ClickHouse pilot schema.
//!
//! Connects to a ClickHouse instance reachable at `CLICKHOUSE_URL`,
//! applies `init.sql` (idempotent — no-op if already applied), then for
//! each of the 17 mirrored tables: inserts one row, reads it back,
//! verifies the round-trip, and deletes the test data via partition
//! drop or `ALTER TABLE … DELETE`.
//!
//! Also exercises the `transaction_hash_dict` Dictionary: inserts a row
//! into `transaction_hash_index`, reloads the dictionary, and checks
//! `dictGet(...)` returns the expected `ledger_sequence`.
//!
//! Gated on `CLICKHOUSE_URL` env: skipped cleanly if unset, so CI
//! without a ClickHouse instance is green.

use db_clickhouse::{Config, apply_init_sql, client};

const SMOKE_LEDGER: i64 = 99_999_001; // out-of-band sentinel to avoid colliding with real data

/// Skip the test cleanly when no ClickHouse is reachable.
fn ch_url() -> Option<String> {
    std::env::var("CLICKHOUSE_URL").ok()
}

#[tokio::test]
async fn smoke_inserts_and_reads_each_table() {
    let Some(url) = ch_url() else {
        eprintln!("CLICKHOUSE_URL not set — skipping smoke test");
        return;
    };

    let cfg = Config {
        url,
        ..Config::from_env()
    };
    let client = client(&cfg);

    apply_init_sql(&client)
        .await
        .expect("apply init.sql must succeed (idempotent)");

    cleanup(&client).await;

    // ----- ledgers (immutable lookup, partitioned) -----
    client
        .query(
            "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) \
             VALUES (?, unhex('0000000000000000000000000000000000000000000000000000000000000001'), now64(3), 22, 1, 100)",
        )
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert ledgers");
    assert_count(&client, "ledgers", &format!("sequence = {SMOKE_LEDGER}"), 1).await;

    // ----- accounts (state) -----
    client
        .query(
            "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain) \
             VALUES (?, 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', ?, ?, 0, NULL)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert accounts");
    assert_count(&client, "accounts", &format!("id = {SMOKE_LEDGER}"), 1).await;

    // ----- assets (state) — composite PK, no surrogate `id` -----
    client
        .query(
            "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url) \
             VALUES (1, 'USDC', ?, 0, 'USD Coin', toDecimal128('1000000.0', 7), 5, NULL)",
        )
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert assets");
    assert_count(
        &client,
        "assets",
        &format!("asset_code = 'USDC' AND issuer_id = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- account_balances_current (state) -----
    client
        .query(
            "INSERT INTO account_balances_current (account_id, asset_type, asset_code, issuer_id, balance, last_updated_ledger) \
             VALUES (?, 1, 'USDC', ?, toDecimal128('123.4567890', 7), ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert account_balances_current");
    assert_count(
        &client,
        "account_balances_current",
        &format!("account_id = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- soroban_contracts (state) -----
    client
        .query(
            "INSERT INTO soroban_contracts (id, contract_id, wasm_hash, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac, name) \
             VALUES (?, 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', unhex('0000000000000000000000000000000000000000000000000000000000000002'), ?, NULL, NULL, NULL, false, NULL)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert soroban_contracts");
    assert_count(
        &client,
        "soroban_contracts",
        &format!("id = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- wasm_interface_metadata (immutable lookup) -----
    client
        .query(
            "INSERT INTO wasm_interface_metadata (wasm_hash, metadata) \
             VALUES (unhex('0000000000000000000000000000000000000000000000000000000000000099'), '{\"functions\":[]}')",
        )
        .execute()
        .await
        .expect("insert wasm_interface_metadata");
    assert_count(
        &client,
        "wasm_interface_metadata",
        "hex(wasm_hash) = '0000000000000000000000000000000000000000000000000000000000000099'",
        1,
    )
    .await;

    // ----- transactions (append-only fact, partitioned) -----
    client
        .query(
            "INSERT INTO transactions (id, hash, ledger_sequence, application_order, source_id, fee_charged, inner_tx_hash, successful, operation_count, has_soroban, parse_error) \
             VALUES (?, unhex('0000000000000000000000000000000000000000000000000000000000000003'), ?, 1, ?, 100, NULL, true, 1, false, false)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert transactions");
    assert_count(
        &client,
        "transactions",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- transaction_hash_index (append-only fact, source for Dictionary) -----
    client
        .query(
            "INSERT INTO transaction_hash_index (hash, ledger_sequence) \
             VALUES (unhex('00000000000000000000000000000000000000000000000000000000000000aa'), ?)",
        )
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert transaction_hash_index");
    assert_count(
        &client,
        "transaction_hash_index",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- operations_appearances (append-only fact) — no surrogate `id` -----
    client
        .query(
            "INSERT INTO operations_appearances (transaction_id, application_order, type, source_id, destination_id, contract_id, asset_code, asset_issuer_id, pool_id, amount, ledger_sequence) \
             VALUES (?, 1, 1, ?, NULL, NULL, '', NULL, NULL, 100, ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert operations_appearances");
    assert_count(
        &client,
        "operations_appearances",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- transaction_participants (append-only fact) -----
    client
        .query(
            "INSERT INTO transaction_participants (account_id, ledger_sequence, transaction_id) \
             VALUES (?, ?, ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert transaction_participants");
    assert_count(
        &client,
        "transaction_participants",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- soroban_events (append-only fact, full-content; the v3 design) -----
    client
        .query(
            "INSERT INTO soroban_events (contract_id, transaction_id, ledger_sequence, event_index, event_type, signature, topics_xdr, data_xdr) \
             VALUES (?, ?, ?, 0, 1, 'transfer', 'topics', 'data')",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert soroban_events");
    assert_count(
        &client,
        "soroban_events",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- soroban_invocations_appearances (append-only fact) -----
    client
        .query(
            "INSERT INTO soroban_invocations_appearances (contract_id, transaction_id, ledger_sequence, caller_id, caller_contract_id, amount) \
             VALUES (?, ?, ?, ?, NULL, 1)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert soroban_invocations_appearances");
    assert_count(
        &client,
        "soroban_invocations_appearances",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- nfts (state) — composite PK, no surrogate `id` -----
    client
        .query(
            "INSERT INTO nfts (contract_id, token_id, collection_name, name, media_url, minted_at_ledger, current_owner_id, current_owner_ledger) \
             VALUES (?, 'tok-1', NULL, NULL, NULL, ?, ?, ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert nfts");
    assert_count(
        &client,
        "nfts",
        &format!("contract_id = {SMOKE_LEDGER} AND token_id = 'tok-1'"),
        1,
    )
    .await;

    // ----- nft_ownership (append-only fact) — composite PK, no `nft_id` -----
    client
        .query(
            "INSERT INTO nft_ownership (contract_id, token_id, ledger_sequence, event_order, transaction_id, owner_id, event_type) \
             VALUES (?, 'tok-1', ?, 0, ?, ?, 0)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert nft_ownership");
    assert_count(
        &client,
        "nft_ownership",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- liquidity_pools (state) — version=last_updated_ledger (task 0208) -----
    client
        .query(
            "INSERT INTO liquidity_pools (pool_id, asset_a_type, asset_a_code, asset_a_issuer_id, asset_b_type, asset_b_code, asset_b_issuer_id, fee_bps, last_updated_ledger) \
             VALUES (unhex('00000000000000000000000000000000000000000000000000000000000000bb'), 0, '', 0, 1, 'USDC', ?, 30, ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert liquidity_pools");
    assert_count(
        &client,
        "liquidity_pools",
        "hex(pool_id) = '00000000000000000000000000000000000000000000000000000000000000BB'",
        1,
    )
    .await;

    // ----- liquidity_pool_snapshots (append-only fact) — no surrogate `id` -----
    client
        .query(
            "INSERT INTO liquidity_pool_snapshots (pool_id, ledger_sequence, reserve_a, reserve_b, total_shares, tvl, volume, fee_revenue) \
             VALUES (unhex('00000000000000000000000000000000000000000000000000000000000000bb'), ?, toDecimal128('100.0', 7), toDecimal128('200.0', 7), toDecimal128('150.0', 7), NULL, NULL, NULL)",
        )
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert liquidity_pool_snapshots");
    assert_count(
        &client,
        "liquidity_pool_snapshots",
        &format!("ledger_sequence = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- lp_positions (state) -----
    client
        .query(
            "INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger) \
             VALUES (unhex('00000000000000000000000000000000000000000000000000000000000000bb'), ?, toDecimal128('50.0', 7), ?, ?)",
        )
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .bind(SMOKE_LEDGER)
        .execute()
        .await
        .expect("insert lp_positions");
    assert_count(
        &client,
        "lp_positions",
        &format!("account_id = {SMOKE_LEDGER}"),
        1,
    )
    .await;

    // ----- transaction_hash_dict Dictionary -----
    // Force the cache to refresh against the row we just inserted.
    client
        .query("SYSTEM RELOAD DICTIONARY transaction_hash_dict")
        .execute()
        .await
        .expect("reload dict");

    let resolved: i64 = client
        .query(
            "SELECT dictGet('transaction_hash_dict', 'ledger_sequence', \
             tuple(toString(unhex('00000000000000000000000000000000000000000000000000000000000000aa'))))",
        )
        .fetch_one()
        .await
        .expect("dictGet");
    assert_eq!(
        resolved, SMOKE_LEDGER,
        "dictGet must resolve hash → ledger_sequence"
    );

    cleanup(&client).await;
}

/// Assert there is exactly `expected` rows in `table` matching `where_clause`.
///
/// No `FINAL`: plain `MergeTree` tables reject it, and the smoke test only
/// inserts one row per (table, sentinel) combination so background
/// `ReplacingMergeTree` dedup never hides anything.
async fn assert_count(client: &clickhouse::Client, table: &str, where_clause: &str, expected: u64) {
    let q = format!("SELECT count() FROM {table} WHERE {where_clause}");
    let actual: u64 = client
        .query(&q)
        .fetch_one()
        .await
        .unwrap_or_else(|e| panic!("count from {table}: {e}"));
    assert_eq!(
        actual, expected,
        "table {table} expected {expected} row(s) for `{where_clause}`, got {actual}"
    );
}

/// Wipe all sentinel rows. Run before and after the test so a previous
/// failed run doesn't poison the next attempt.
async fn cleanup(client: &clickhouse::Client) {
    let l = SMOKE_LEDGER;
    let stmts = [
        format!("ALTER TABLE ledgers DELETE WHERE sequence = {l}"),
        format!("ALTER TABLE accounts DELETE WHERE id = {l}"),
        format!("ALTER TABLE assets DELETE WHERE issuer_id = {l} OR contract_id = {l}"),
        format!("ALTER TABLE account_balances_current DELETE WHERE account_id = {l}"),
        format!("ALTER TABLE soroban_contracts DELETE WHERE id = {l}"),
        "ALTER TABLE wasm_interface_metadata DELETE WHERE hex(wasm_hash) = '0000000000000000000000000000000000000000000000000000000000000099'".into(),
        format!("ALTER TABLE transactions DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE transaction_hash_index DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE operations_appearances DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE transaction_participants DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE soroban_events DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE soroban_invocations_appearances DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE nfts DELETE WHERE contract_id = {l}"),
        format!("ALTER TABLE nft_ownership DELETE WHERE ledger_sequence = {l}"),
        "ALTER TABLE liquidity_pools DELETE WHERE hex(pool_id) = '00000000000000000000000000000000000000000000000000000000000000BB'".into(),
        format!("ALTER TABLE liquidity_pool_snapshots DELETE WHERE ledger_sequence = {l}"),
        format!("ALTER TABLE lp_positions DELETE WHERE account_id = {l}"),
    ];
    for s in stmts {
        // Ignore errors: cleanup runs on a freshly-created schema where the
        // tables exist but rows might not. ALTER … DELETE is async and
        // never errors on no-match.
        let _ = client
            .query(&s)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }
}
