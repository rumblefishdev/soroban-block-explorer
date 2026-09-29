use super::key_and_partition_lists;

#[test]
fn a_page_inside_one_partition_emits_that_partition_once() {
    let (keys, partitions) = key_and_partition_lists(&[63_903_900, 63_903_901, 63_903_902]);

    assert_eq!(keys, "63903900,63903901,63903902");
    assert_eq!(
        partitions, "127",
        "twenty repeats of the same partition would defeat the prune's purpose"
    );
}

#[test]
fn a_page_straddling_a_boundary_emits_both_partitions_in_order() {
    let (_, partitions) = key_and_partition_lists(&[63_999_999, 64_000_000]);

    assert_eq!(partitions, "127,128");
}

#[test]
fn one_ledger_is_the_detail_paths_shape() {
    let (keys, partitions) = key_and_partition_lists(&[63_903_902]);

    assert_eq!(keys, "63903902");
    assert_eq!(partitions, "127");
}

#[test]
fn keys_keep_page_order_and_duplicates_are_the_callers_problem() {
    // `fetch_list` dedups before calling, so this only pins that the key
    // list is a faithful echo — collapsing here would silently mask a
    // caller that stopped deduping.
    let (keys, _) = key_and_partition_lists(&[3, 1, 3]);

    assert_eq!(keys, "3,1,3");
}
