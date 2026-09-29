use super::*;

#[test]
fn split_statements_drops_line_comments_and_empty_chunks() {
    let sql = "-- top comment\n\
               CREATE TABLE a (x Int64) ENGINE = MergeTree ORDER BY x;\n\
               -- mid comment\n\
               CREATE TABLE b (y Int64) ENGINE = MergeTree ORDER BY y;\n\
               ;\n";
    let stmts = split_statements(sql);
    assert_eq!(stmts.len(), 2);
    assert!(stmts[0].starts_with("CREATE TABLE a"));
    assert!(stmts[1].starts_with("CREATE TABLE b"));
}

#[test]
fn init_sql_parses_into_statements() {
    let stmts = split_statements(INIT_SQL);
    // Current: 25 CREATE TABLE + 1 CREATE MATERIALIZED VIEW + 1 CREATE DICTIONARY
    // = 27 (matches the assertion below). Historical derivation:
    // 17-table base; task 0217 added `nfts_pending` + `nft_ownership_pending`
    // as schema-only landing zones (the CH writer does NOT yet stage/INSERT
    // into either — follow-up to PR #180); task 0231 (ADR 0050) added the
    // `asset_enrichment` + `nft_enrichment` side tables; lore-0293 added the
    // `asset_aggregates` table + its refreshable `asset_aggregates_mv` for
    // pre-computed asset aggregates (`assets.total_supply` / `holder_count`
    // now served from `asset_aggregates`; the dead columns were dropped in
    // task 0310); task 0297 added the `soroban_contract_metadata` side
    // table (on-chain token name/symbol/decimals); ADR 0051 / task 0339 added
    // the `asset_sac` SAC-facet side table; task 0331 FIRST added an INTERIM
    // `soroban_token_balances` per-holder side table (asset_type=3 balances
    // from ContractData `Balance(Address)` ledger state), then
    // task 0331 Option C: unified `balances` + `balance_aggregates`
    // (+ its refreshable MV) REPLACED that interim `soroban_token_balances` +
    // `soroban_asset_aggregates` (+ MV) — net one fewer table, same MV count.
    // (The interim `addresses` resolution table was dropped — holder→StrKey
    // resolves via `accounts` (G) / `soroban_contracts` (C), no dimension.)
    // task 0331 simplification: DROPPED the legacy `asset_aggregates` table +
    // `asset_aggregates_mv` (classic supply over `account_balances_current`) —
    // superseded by `balance_aggregates` over `balances`. 29 → 27.
    // task 0331 Option A: DROPPED `soroban_token_supply` (the per-token
    // `TotalSupply` key read) — `balance_aggregates` (Σ amount) is the sole
    // supply source; one universal method, no seed-only staleness. 27 → 26.
    // ADR 0051 / task 0339 (merged): added the `asset_sac` SAC-facet side table
    // (SAC-ness of a classic/native asset, not a distinct row). 26 → 27.
    // task 0359: added `operation_asset_appearances` — per-(asset, tx) presence
    // index (asset-leading key; native first-class). 27 → 28.
    // task 0365: added `operation_pools` — per-(pool, tx) presence index
    // (pool-leading key; the pool-dimension twin of the above). 28 → 29.
    // task 0385: added `accounts_recent` (last_seen_ledger-ordered read-model for
    // the acclist browse) + its refreshable `accounts_recent_mv`. 29 → 31.
    // task 0279: added `lp_operation_amounts` — per-(op, pool, asset) amounts
    // behind the pool page's "Amount" column (issue #371). 31 → 32.
    // task 0463: added `account_entry_state` — signers + thresholds side table
    // (issue #377); side table, not columns on `accounts`, because that
    // table takes whole-row writes from more than one path. 33 → 34:
    // pool_instance_state (task 0374 step 15, the asset_sac-pattern side
    // table: what a pool declares about itself) and pool_state_changes
    // (step 7, plane-state reserves).
    // 34 → 35.
    // task 0540 / 0541: added `asset_transfers` (one row per token
    // movement, the lossless replacement for `net_settled`),
    // `transaction_memos` (memo per transaction, from the envelope) and
    // `soroban_event_ops` (operation attribution per event, the side table
    // `soroban_events` cannot take). 35 → 38.
    // task 0548: added `contract_executable_refs` — what an owner's
    // executable tag points at (CAP-85). 38 → 39.
    // task 0210: added `claimable_balance_holdings` — `balances`' twin for
    // value held by a claimable balance. 39 → 40.
    // task 0541: dropped `soroban_event_ops` — the operation is part of
    // the `soroban_events` key (ADR 0059). 40 → 39.
    // task 0541: added `contract_transactions` — the per-(contract, tx)
    // presence index the contract-filtered transaction list seeks. 39 → 40.
    // task 0396: dropped `transaction_hash_dict` — never called. 40 → 39.
    // task 0580: added `transaction_hash_prefix_index` — the hash index
    // keyed by an 8-byte prefix. 39 → 40.
    // task 0374: added `pool_activity` + its refreshable MV — the soroban
    // pool's last reserve change, the pool list's order key. 40 → 42.
    // task 0580: dropped `transaction_hash_index` — the readers moved to
    // `transaction_hash_prefix_index`. 42 → 41.
    // task 0372: added `transaction_operations` and
    // `pool_operation_amounts` (the operation tables located by the
    // transaction position), dropped `operation_pools` (no reader since
    // task 0491). 41 → 42.
    // task 0372: dropped `operations_appearances` and
    // `lp_operation_amounts` — every reader moved to the tables above.
    // 42 → 40.
    // task 0586: added `contract_activity` — `contract_transactions` and
    // the invocations' callers in one position-keyed table. 40 → 41.
    // task 0586: dropped `contract_transactions` and
    // `soroban_invocations_appearances` — every reader moved to
    // `contract_activity`. 41 → 39.
    // task 0424: added `nft_ownership_changes` + `_pending` — the
    // ownership rows located by their event. 39 → 41.
    // task 0424: dropped `nft_ownership` + `_pending` — every reader moved
    // to `nft_ownership_changes`. 41 → 39.
    // task 0374: added `soroban_pool_event_amounts` — per-(event, leg) soroban
    // pool amounts. 39 → 40.
    assert_eq!(
        stmts.len(),
        40,
        "expected 37 tables + 3 materialized views, got {}",
        stmts.len()
    );
}
