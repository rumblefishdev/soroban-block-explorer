//! The load-bearing clauses of the balance-change statement, asserted against
//! the built SQL — each is one word away from a regression that review does not
//! catch, and none of them needs a ClickHouse to pin. The wire-type contract
//! that DOES need a server lives in `decode_smoke` beside them.

use super::*;

#[test]
fn a_transfer_to_self_nets_to_zero() {
    // `to − from`, never a first-match branch: with `from_id = to_id = A` a
    // `multiIf` that tests `to_id` first returns `+amount` and the page shows
    // an account paying itself. Both legs must fire and cancel.
    let sql = balance_change_delta_sql(42, "(1,1)", "0");
    assert!(
        sql.contains("if(to_id = 42, amount, 0) - if(from_id = 42, amount, 0)"),
        "signed amount must subtract the outgoing leg, not branch on it: {sql}"
    );
}

#[test]
fn the_read_dedups_and_prunes() {
    let sql = balance_change_delta_sql(42, "(1,1)", "0");
    // `asset_transfers` is a version-less ReplacingMergeTree and production
    // tables here carry unmerged duplicates permanently (0420) — without the
    // inner group-by over the FULL sort key a duplicate doubles the amount.
    assert!(
        sql.contains(
            "GROUP BY ledger_sequence, application_order, op_index, event_pos_in_op, asset_id"
        ),
        "inner dedup over the full sort key is missing: {sql}"
    );
    // PARTITION BY intDiv(ledger_sequence, 500000) — the key filter alone
    // does not prune.
    assert!(
        sql.contains("intDiv(ledger_sequence, 500000) IN"),
        "partition prune is missing: {sql}"
    );
}

#[test]
fn a_non_fungible_movement_never_sums_as_zero() {
    let sql = balance_change_delta_sql(42, "(1,1)", "0");
    // A `{token_id}` movement has no amount by nature. It is counted, signed,
    // in its own column; folding it into the amount sum would print a real
    // "balance change: 0" for a transaction that changed an owner.
    assert!(
        sql.contains("if(amount IS NULL,"),
        "non-fungible movements must be counted separately: {sql}"
    );
    assert!(
        sql.contains("sum(nft) AS nft_delta"),
        "non-fungible count must survive the outer aggregate: {sql}"
    );
    // NULL and 0 are different answers, so the HAVING must not conflate them.
    assert!(
        sql.contains("HAVING (delta IS NOT NULL AND delta != '0') OR nft_delta != 0"),
        "HAVING must keep non-fungible rows and drop only measured zeros: {sql}"
    );
}

#[test]
fn assets_come_back_in_the_order_they_moved() {
    // Assets have different decimals and different prices, and we hold no
    // prices at all — so any ranking by amount compares quantities that are
    // not comparable. The order the movements HAPPENED in is a fact the chain
    // supplies, and it is the one the cell shows (`[0]` plus `+N`), so it has
    // to come from the statement and must not be re-sorted afterwards.
    let sql = balance_change_delta_sql(42, "(1,1)", "0");
    assert!(
        sql.contains(
            "ORDER BY ledger_sequence, application_order, min((op_index, event_pos_in_op))"
        ),
        "assets must be ordered by their first movement in the transaction: {sql}"
    );
}

#[test]
fn nft_pieces_are_attributed_by_their_previous_owner() {
    let pieces = HashMap::from([
        ((7, 99, Some(2), Some(1)), vec!["44".into(), "45".into()]),
        ((7, 99, Some(2), Some(3)), vec!["99".into()]),
    ]);

    assert_eq!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), -2, 1),
        vec!["44", "45"],
        "Alice must get only the two pieces she previously owned"
    );
    assert_eq!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), -1, 3),
        vec!["99"],
        "Carol must get only the piece she previously owned"
    );
    assert_eq!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), 3, 2),
        vec!["44", "45", "99"],
        "Bob received every piece in the new-owner group"
    );
}

#[test]
fn nft_piece_ids_stay_hidden_without_a_complete_proof() {
    let pieces = HashMap::from([
        ((7, 99, Some(2), Some(1)), vec!["44".into()]),
        ((7, 99, Some(2), None), vec!["45".into()]),
    ]);

    assert!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), -2, 1).is_empty(),
        "an outgoing group with one unknown previous owner must stay collapsed"
    );
    assert!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), 3, 2).is_empty(),
        "an incoming group whose piece count disagrees must stay collapsed"
    );
}

#[test]
fn nft_mints_and_burns_keep_the_same_owner_history_rule() {
    let pieces = HashMap::from([
        ((7, 100, Some(2), None), vec!["101".into()]),
        ((7, 101, None, Some(1)), vec!["102".into()]),
    ]);

    assert_eq!(
        verified_piece_ids(&pieces, (7, 100, Some(2)), 1, 2),
        vec!["101"],
        "a mint has no previous owner and belongs to its recipient"
    );
    assert_eq!(
        verified_piece_ids(&pieces, (7, 101, None), -1, 1),
        vec!["102"],
        "a burn has no new owner and belongs to its previous owner"
    );
}

#[test]
fn nft_piece_proof_is_isolated_and_counts_unique_pieces() {
    let pieces = HashMap::from([
        ((7, 99, Some(2), Some(1)), vec!["44".into(), "44".into()]),
        ((7, 99, Some(2), Some(3)), vec!["99".into()]),
        ((7, 100, Some(2), Some(1)), vec!["unrelated-tx".into()]),
        (
            (8, 99, Some(2), Some(1)),
            vec!["unrelated-collection".into()],
        ),
    ]);

    assert_eq!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), -1, 1),
        vec!["44"],
        "duplicates and unrelated lookups must not inflate Alice's proof"
    );
    assert!(
        verified_piece_ids(&pieces, (7, 99, Some(2)), 3, 2).is_empty(),
        "two unique candidates cannot prove three incoming movements"
    );
}
