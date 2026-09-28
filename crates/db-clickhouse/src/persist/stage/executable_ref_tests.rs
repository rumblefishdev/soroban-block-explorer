use super::*;

fn target(tag: &str, hash_byte: &str, ledger: u32) -> ExtractedExecutableRefTarget {
    ExtractedExecutableRefTarget {
        owner: "COWNER".to_string(),
        tag: tag.to_string(),
        wasm_hash: hash_byte.repeat(32),
        ledger_sequence: ledger,
    }
}

/// The parser folds per transaction, so two transactions of one ledger that
/// re-point the same tag reach the stage as two targets with the same version.
/// Only the later one may be written — an equal-version pair would leave the
/// survivor to insert order.
#[test]
fn two_transactions_re_pointing_one_tag_in_one_ledger_write_only_the_last() {
    let rows = build_executable_ref_rows(&[
        target("fleet", "aa", 100), // tx 1
        target("fleet", "bb", 100), // tx 2, same ledger
    ]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].wasm_hash, [0xbb; 32]);
}

#[test]
fn different_tags_and_different_ledgers_are_all_kept() {
    let rows = build_executable_ref_rows(&[
        target("fleet", "aa", 100),
        target("other", "bb", 100),
        target("fleet", "cc", 101),
    ]);
    assert_eq!(rows.len(), 3);
}
