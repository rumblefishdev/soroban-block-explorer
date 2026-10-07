use super::*;

#[test]
fn contract_type_name_matches_pg_function() {
    assert_eq!(contract_type_name(0).as_deref(), Some("token"));
    assert_eq!(contract_type_name(1).as_deref(), Some("other"));
    assert_eq!(contract_type_name(2).as_deref(), Some("nft"));
    assert_eq!(contract_type_name(3).as_deref(), Some("fungible"));
    assert_eq!(contract_type_name(4), None);
}

#[test]
fn map_upgradeable_three_state() {
    // SAC → Immutable regardless of the join code: nothing to swap.
    assert_eq!(map_upgradeable(false, true, -1), Some(false));
    assert_eq!(map_upgradeable(false, true, 1), Some(false));
    // WASM present: 1 → upgradeable, 0 → frozen.
    assert_eq!(map_upgradeable(true, false, 1), Some(true));
    assert_eq!(map_upgradeable(true, false, 0), Some(false));
    // WASM present, -1 (no metadata row / pre-0327 key absent) → Unknown.
    assert_eq!(map_upgradeable(true, false, -1), None);
}

/// Task 0548 — the case that used to answer a confident "cannot upgrade"
/// about a contract we know nothing about. Covers both populations: a
/// pre-0548 placeholder row (no deploy observed) and, from protocol 28, a
/// contract whose code is owned by another contract.
#[test]
fn no_wasm_and_not_a_sac_is_unknown_not_immutable() {
    assert_eq!(map_upgradeable(false, false, -1), None);
    assert_eq!(
        map_upgradeable(false, false, 1),
        None,
        "an executable we never resolved cannot be reported as frozen or as \
         upgradeable — the chip must stay off"
    );
}

/// The wire label must stay derived from the constant, so the number the
/// SQL windows on and the string the client is told can never disagree.
#[test]
fn wire_label_is_derived_from_the_window_constant() {
    assert_eq!(stats_window_label(), format!("{STATS_WINDOW_DAYS} days"));
    assert!(stats_window_label().starts_with(&STATS_WINDOW_DAYS.to_string()));
}

// Regression guard for task 0300: CH `recent_events` was hardcoded `0`.
// The stats SQL MUST select a real windowed event count off `soroban_events`
// (parity with PG's appearance-fold SUM), not a literal.
#[test]
fn stats_sql_computes_recent_events_from_events_table() {
    let sql = contract_stats_sql(7);

    assert!(
        sql.contains("AS recent_events"),
        "recent_events column missing: {sql}"
    );
    assert!(
        sql.contains("FROM soroban_events se"),
        "recent_events must read soroban_events: {sql}"
    );
    // The bug shape: a bare literal aliased to recent_events. Collapse
    // whitespace first so the guard is alignment-independent.
    let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !normalized.contains("0 AS recent_events"),
        "recent_events still hardcoded to a literal: {sql}"
    );
    // Window parity: both the invocations seek and the events subquery
    // apply the same ledger floor + INTERVAL N DAY bound.
    assert_eq!(
        sql.matches("INTERVAL 7 DAY").count(),
        2,
        "events window must mirror the invocations window: {sql}"
    );
    // Two binds: events-subquery contract_id, then outer contract_id.
    assert_eq!(
        sql.matches("contract_id = ?").count(),
        2,
        "expected two `contract_id = ?` binds: {sql}"
    );
    // The scalar subquery MUST be `ifNull(…, 0)`-wrapped: CH types a bare
    // `(SELECT …)` as Nullable(UInt64), which fails the non-nullable `u64`
    // decode → 500 on every contract detail.
    assert!(
        normalized.contains("ifNull(( SELECT toUInt64(count())"),
        "recent_events subquery must be ifNull-wrapped: {sql}"
    );
}

/// Regression guard for lore-0420. Two failure modes, one shape.
///
/// `ledgers` is a ReplacingMergeTree with unmerged duplicate rows, so
/// JOINing it into a `count()` multiplies the count by the number of
/// physical copies (measured ~1.6x). And the seek bound must be resolved
/// from the data, not from a hardcoded ledgers-per-day constant: the old
/// `days * 17_280` assumed a 5 s cadence, ran 13% wide against the real
/// ~5.6 s, and would silently run SHORT — under-reporting the window — if
/// the chain ever sped up.
///
/// One `min(sequence)` bound satisfies both: immune to duplicates, exact by
/// construction.
#[test]
fn stats_sql_bounds_window_from_data_never_a_join_or_a_constant() {
    let sql = contract_stats_sql(7);
    let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        !normalized.contains("JOIN ledgers"),
        "a JOIN onto ledgers fans each row out per duplicate copy and \
         inflates the count: {sql}"
    );
    assert!(
        !normalized.contains("17280") && !normalized.contains("17_280"),
        "the window bound must come from the data, not a ledgers-per-day \
         constant that drifts with the chain cadence: {sql}"
    );
    // One per window: the invocations seek and the events subquery.
    assert_eq!(
        normalized
            .matches("SELECT min(sequence) FROM ledgers WHERE closed_at >=")
            .count(),
        2,
        "both the invocations and events windows must derive their bound \
         from the data: {sql}"
    );
}

// Task 0487: a contract called only by other contracts reported 0 unique
// callers, because the count read `caller_id` (accounts) alone. The pair
// counts both; `coalesce` would merge two id spaces that can collide.
#[test]
fn stats_sql_counts_account_and_contract_callers() {
    let sql = contract_stats_sql(7);
    assert!(
        sql.contains("uniqExact(tuple(ca.caller_id, ca.caller_contract_id))"),
        "unique callers must count both caller columns: {sql}"
    );
}
