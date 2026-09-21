use super::*;

fn synthetic_ledger() -> ExtractedLedger {
    ExtractedLedger {
        sequence: 1,
        hash: "00".repeat(32),
        closed_at: 0,
        protocol_version: 22,
        transaction_count: 0,
        base_fee: 100,
    }
}

/// `persist_ledger_clickhouse` against an unroutable URL must
/// surface a transport error — we treat it as the "no side effect"
/// proof that the wrapper actually issues the insert path
/// (the stub it replaced returned Ok without any I/O).
#[tokio::test]
async fn wrapper_returns_err_when_client_unreachable() {
    // Deliberately unroutable URL — any real network use must fail.
    let client = Client::default().with_url("http://127.0.0.1:1");
    let ledger = synthetic_ledger();
    // Single ledger, no transactions or downstream data — the
    // writer still opens the `ledgers` insert at commit time, which
    // touches the network and surfaces the unroutable-URL error.
    let res = persist_ledger_clickhouse(
        &client,
        &ledger,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &ClassificationCache::new(),
    )
    .await;
    assert!(res.is_err(), "expected transport error, got: {res:?}");
}
