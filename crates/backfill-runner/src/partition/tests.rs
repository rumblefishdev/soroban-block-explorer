use super::*;

#[test]
fn partition_bounds() {
    let p = Partition::from_ledger(62_026_937);
    assert_eq!(p.start, 62_016_000);
    assert_eq!(p.end, 62_079_999);
    assert_eq!(p.hex, "FC4DB5FF");
}

/// Ledgers 0 and 1 exist in no archive, so the genesis partition holds
/// two files fewer than every other.
#[test]
fn genesis_partition_starts_at_ledger_two() {
    let p = Partition::from_ledger(0);
    assert_eq!(p.first_ledger(), 2);
    assert_eq!(p.ledger_count(), 63_998);
    assert_eq!(p.clamped(0, 100), (2, 100));
}

#[test]
fn a_later_partition_holds_partition_size_ledgers() {
    let p = Partition::from_ledger(62_026_937);
    assert_eq!(p.first_ledger(), 62_016_000);
    assert_eq!(p.ledger_count(), PARTITION_SIZE as usize);
    assert_eq!(p.clamped(0, u32::MAX), (62_016_000, 62_079_999));
}

#[test]
fn partition_folder_key() {
    let p = Partition::from_ledger(62_026_937);
    assert_eq!(
        p.folder_key(),
        "v1.1/stellar/ledgers/pubnet/FC4DB5FF--62016000-62079999"
    );
}

/// First Soroban-era ledger (Protocol 20 go-live, 2024-02-20). Guards
/// against off-by-one in the hex math at a known-good real sequence.
#[test]
fn soroban_start_ledger_partition() {
    let p = Partition::from_ledger(50_457_424);
    assert_eq!(p.start, 50_432_000);
    assert_eq!(p.end, 50_495_999);
    assert_eq!(p.hex, "FCFE77FF");
}

/// A sequence exactly on a partition boundary must land in the new
/// partition (start == seq), not the previous one.
#[test]
fn partition_boundary_start_is_inclusive() {
    let p = Partition::from_ledger(62_016_000);
    assert_eq!(p.start, 62_016_000);
    assert_eq!(p.end, 62_079_999);
}

/// A sequence at the last slot of a partition must stay in that
/// partition, not roll forward.
#[test]
fn partition_boundary_end_is_inclusive() {
    let p = Partition::from_ledger(62_079_999);
    assert_eq!(p.start, 62_016_000);
    assert_eq!(p.end, 62_079_999);
}

/// Adjacent ledgers that straddle a partition boundary must produce
/// different partitions — catches modular-arithmetic regressions.
#[test]
fn adjacent_ledgers_across_boundary_differ() {
    let prev = Partition::from_ledger(62_015_999);
    let next = Partition::from_ledger(62_016_000);
    assert_ne!(prev, next);
    assert_eq!(prev.end + 1, next.start);
}

#[test]
fn s3_folder_has_scheme_bucket_and_trailing_slash() {
    let p = Partition::from_ledger(62_026_937);
    assert_eq!(
        p.s3_folder(),
        "s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/FC4DB5FF--62016000-62079999/"
    );
}

#[test]
fn local_folder_under_temp_dir() {
    let p = Partition::from_ledger(62_026_937);
    let temp = Path::new("/var/tmp/backfill");
    assert_eq!(
        p.local_folder(temp),
        PathBuf::from("/var/tmp/backfill/FC4DB5FF--62016000-62079999")
    );
}

#[test]
fn local_ledger_path_joins_partition_and_file_name() {
    let p = Partition::from_ledger(62_026_937);
    let temp = Path::new("/var/tmp/backfill");
    assert_eq!(
        p.local_ledger_path(62_026_937, temp),
        PathBuf::from("/var/tmp/backfill/FC4DB5FF--62016000-62079999/FC4D8B46--62026937.xdr.zst")
    );
}

#[test]
fn partitions_for_range_single_partition_when_both_bounds_inside() {
    let ps = partitions_for_range(62_020_000, 62_025_000);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].start, 62_016_000);
    assert_eq!(ps[0].end, 62_079_999);
}

#[test]
fn partitions_for_range_start_equal_end_yields_one() {
    let ps = partitions_for_range(50_457_424, 50_457_424);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].start, 50_432_000);
}

#[test]
fn partitions_for_range_spans_three_partitions() {
    // Inside first, across second, into third.
    let ps = partitions_for_range(62_020_000, 62_150_000);
    assert_eq!(ps.len(), 3);
    assert_eq!(ps[0].start, 62_016_000);
    assert_eq!(ps[1].start, 62_080_000);
    assert_eq!(ps[2].start, 62_144_000);
}

#[test]
fn partitions_for_range_end_exactly_on_boundary() {
    // end = last ledger of a partition → that partition is included, not
    // the next one.
    let ps = partitions_for_range(62_016_000, 62_079_999);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].end, 62_079_999);
}

#[test]
fn partitions_for_range_end_just_past_boundary_includes_next() {
    let ps = partitions_for_range(62_016_000, 62_080_000);
    assert_eq!(ps.len(), 2);
    assert_eq!(ps[1].start, 62_080_000);
}

#[test]
fn partitions_for_range_empty_when_start_gt_end() {
    let ps = partitions_for_range(100, 50);
    assert!(ps.is_empty());
}
