use super::*;

fn arm(p: &[Position], truncated: bool) -> ArmWindow {
    ArmWindow {
        positions: p.to_vec(),
        truncated,
        last_ledger: if truncated {
            p.iter().map(|x| x.0).min()
        } else {
            None
        },
    }
}

#[test]
fn merges_in_execution_order_and_dedups() {
    let r = merge_arm_windows(
        &[
            arm(&[(10, 3), (10, 1)], false),
            arm(&[(10, 3), (9, 7)], false),
        ],
        Direction::Next,
        10,
    );
    // Next = newest first.
    assert_eq!(r, MergeResult::Page(vec![(10, 3), (10, 1), (9, 7)]));
}

#[test]
fn a_truncated_arm_caps_the_page_at_its_last_complete_ledger() {
    // Arm 1 reached only ledger 10; arm 2 has a row at ledger 8, and arm 1 may
    // hold rows between 10 and 8 it never read.
    let r = merge_arm_windows(
        &[arm(&[(11, 2), (10, 5)], true), arm(&[(8, 1)], false)],
        Direction::Next,
        2,
    );
    assert_eq!(r, MergeResult::Page(vec![(11, 2), (10, 5)]));
}

#[test]
fn asks_for_a_wider_window_when_the_cap_leaves_too_few() {
    let r = merge_arm_windows(
        &[arm(&[(11, 2)], true), arm(&[(8, 1), (7, 1)], false)],
        Direction::Next,
        3,
    );
    assert_eq!(r, MergeResult::NeedWiderWindow(vec![(11, 2)]));
}

#[test]
fn ascending_pages_cap_at_the_lowest_truncated_ledger() {
    let truncated = ArmWindow {
        positions: vec![(5, 1), (6, 2)],
        truncated: true,
        last_ledger: Some(6),
    };
    let r = merge_arm_windows(&[truncated, arm(&[(9, 1)], false)], Direction::Prev, 2);
    assert_eq!(r, MergeResult::Page(vec![(5, 1), (6, 2)]));
}

#[test]
fn window_sql_reads_the_arm_in_ledger_order_from_the_cursor_ledger() {
    let sql = window_last_ledger_sql(
        Arm::Events,
        42,
        "intDiv(ledger_sequence, 500000) = 128",
        "64000000",
        Some(64_000_000),
        Direction::Next,
        84,
    );
    assert!(sql.contains("FROM soroban_events WHERE contract_id = 42"));
    assert!(sql.contains("AND ledger_sequence <= 64000000"));
    assert!(sql.ends_with("ORDER BY ledger_sequence DESC LIMIT 1 OFFSET 83"));
}

#[test]
fn window_rows_are_bounded_on_both_ledgers_and_name_positions_for_events() {
    let sql = window_rows_sql(
        Arm::Events,
        42,
        "p",
        "h",
        Some(100),
        Some(90),
        Direction::Next,
    );
    assert!(
        sql.starts_with("SELECT DISTINCT ledger_sequence, application_order FROM soroban_events")
    );
    assert!(sql.contains("AND ledger_sequence <= 100 AND ledger_sequence >= 90"));
    assert!(!sql.contains("transaction_id"));

    let sql = window_rows_sql(Arm::Invocations, 42, "p", "h", None, None, Direction::Prev);
    assert!(sql.starts_with(
        "SELECT DISTINCT ledger_sequence, transaction_id FROM soroban_invocations_appearances"
    ));
    assert!(!sql.contains(">="));
}

#[test]
fn ids_map_to_positions_by_a_transactions_key_seek() {
    let sql = positions_by_id_sql(&[(10, -5), (9, 7), (10, 3)]);
    assert!(sql.contains(
        "WHERE ledger_sequence IN (9,10) AND (ledger_sequence, id) IN ((10,-5),(9,7),(10,3))"
    ));
}
