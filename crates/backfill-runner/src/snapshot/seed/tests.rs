use super::*;

/// The guard reads the session's real settings: a profile that returns partial
/// results must stop the seed, an all-`throw` one must not. Gated on
/// `CLICKHOUSE_URL` like the other seed tests; reads only.
#[tokio::test]
async fn a_profile_that_can_truncate_a_read_stops_the_seed() {
    if std::env::var("CLICKHOUSE_URL").is_err() {
        eprintln!("CLICKHOUSE_URL not set — skipping truncation guard integration test");
        return;
    }
    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());

    refuse_if_reads_can_truncate(&Sink::new(client.clone()))
        .await
        .expect("a default profile throws on every limit");

    let cutting = Sink::new(client.with_setting("result_overflow_mode", "break"));
    let err = refuse_if_reads_can_truncate(&cutting)
        .await
        .expect_err("break returns a partial result as a success");
    assert!(err.to_string().contains("result_overflow_mode"), "{err}");
}
