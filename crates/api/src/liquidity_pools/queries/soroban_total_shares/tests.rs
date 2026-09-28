use super::*;

fn stored(raw: &str, decimals: Option<u32>) -> StoredTotalShares {
    StoredTotalShares {
        raw: raw.to_string(),
        decimals,
    }
}

fn reserves(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_positive_total_is_scaled_by_the_share_token() {
    assert_eq!(
        served_total_shares(
            Some(&stored("1215785565496", Some(7))),
            &reserves(&["5", "9"])
        )
        .as_deref(),
        Some("121578.5565496")
    );
}

/// A stored `0` is a measurement only when the pool holds nothing; while it
/// holds reserves it means the instance has no `TotalShares` key.
#[test]
fn zero_is_served_only_for_an_empty_pool() {
    let zero = stored("0", Some(7));
    assert_eq!(
        served_total_shares(Some(&zero), &reserves(&["0", "0"])).as_deref(),
        Some("0")
    );
    assert_eq!(
        served_total_shares(Some(&zero), &reserves(&["0", "12"])),
        None
    );
    // No reserves known at all: nothing proves the pool is empty.
    assert_eq!(served_total_shares(Some(&zero), &[]), None);
}

#[test]
fn unknown_scale_or_no_row_reads_null() {
    assert_eq!(
        served_total_shares(Some(&stored("4622", None)), &reserves(&["1", "1"])),
        None
    );
    assert_eq!(served_total_shares(None, &reserves(&["0", "0"])), None);
}
