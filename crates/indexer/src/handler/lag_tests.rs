#[test]
fn ingestion_lag_clamps_and_computes() {
    assert_eq!(super::ingestion_lag_secs(1_000, 970), 30);
    assert_eq!(super::ingestion_lag_secs(1_000, 1_005), 0); // clock skew → 0
}
