use super::{LedgerListRow, dedup_consecutive};

fn row(sequence: i64) -> LedgerListRow {
    LedgerListRow {
        sequence,
        hash: format!("h{sequence}"),
        closed_at: sequence * 1000,
        protocol_version: 27,
        transaction_count: 1,
        base_fee: 100,
    }
}

/// The lore-0420 bug: the RMT hands back each sequence 2–3× and the page
/// rendered them all, which also collided the frontend's `sequence` row key.
/// A full page must come back as DISTINCT sequences in the original order.
#[test]
fn collapses_duplicate_sequences_and_fills_the_page() {
    // Desc page as the RMT physically returns it: every sequence doubled,
    // one tripled — over-fetched at ×3 (limit 4 → 12 raw rows).
    let raw = [100, 100, 99, 99, 98, 98, 98, 97, 97, 96, 96, 95]
        .into_iter()
        .map(row)
        .collect();

    let out = dedup_consecutive(raw, 4);

    assert_eq!(
        out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
        vec![100, 99, 98, 97],
        "duplicates must collapse and the page must still be full"
    );
}

/// Under-fill is survivable: a short page is fine, but it must never emit a
/// duplicate (that is what broke React reconciliation).
#[test]
fn never_emits_duplicates_even_when_underfilled() {
    let raw = [50, 50, 50, 49, 49, 49].into_iter().map(row).collect();

    let out = dedup_consecutive(raw, 4);

    assert_eq!(
        out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
        vec![50, 49]
    );
}

/// Pins the precondition rather than hiding it: this collapses ADJACENT
/// duplicates only, so unordered input passes duplicates straight through.
///
/// That is deliberate. Ordering is not an incidental detail of this page —
/// the keyset cursor is read off the last row, so losing `ORDER BY sequence`
/// corrupts pagination itself. A set-based dedup would keep this test green
/// while the page silently returned rows in the wrong order under a wrong
/// cursor; failing here is the cheaper outcome.
#[test]
fn unsorted_input_is_not_deduplicated() {
    // Same three sequences as the sorted case, interleaved.
    let raw = [100, 99, 100, 99, 98, 98].into_iter().map(row).collect();

    let out = dedup_consecutive(raw, 6);

    assert_eq!(
        out.iter().map(|r| r.sequence).collect::<Vec<_>>(),
        vec![100, 99, 100, 99, 98],
        "only the adjacent 98/98 pair collapses — the interleaved duplicates \
         survive, because ordering is the caller's contract"
    );
}
