//! Join the harness `client.csv` (per-request client side) with the SSH-pulled
//! `query_log_per_request.csv` (per-request ClickHouse read_rows/read_bytes)
//! into one `results.csv` — one dry row per HTTP request, ready for stats in
//! post-processing (task 0338, Step 6).
//!
//! LEFT join on `request_id`: every client row is kept; the CH columns are
//! blank for a request that issued no CH query (a cache hit). The `csv` crate
//! handles the quoted `url` column and ClickHouse's fully-quoted CSVWithNames.
//!
//!   cargo run -p load-tests --bin join -- \
//!     --client     .temp/load-tests/run-1000/client.csv \
//!     --query-log  .temp/load-tests/run-1000/query_log_per_request.csv \
//!     --out        .temp/load-tests/run-1000/results.csv

use std::collections::HashMap;

use clap::Parser;

#[derive(Parser)]
#[command(about = "Join client.csv + query_log_per_request.csv on request_id (task 0338)")]
struct Args {
    /// Harness per-request CSV.
    #[arg(long, default_value = "client.csv")]
    client: String,
    /// ClickHouse per-request CSV (query_log_per_request.csv).
    #[arg(long, default_value = "query_log_per_request.csv")]
    query_log: String,
    /// Output CSV.
    #[arg(long, default_value = "results.csv")]
    out: String,
}

fn main() {
    let args = Args::parse();

    // request_id -> [read_rows, read_bytes, ch_queries, ch_duration_ms, memory_max]
    let mut ql: HashMap<String, [String; 5]> = HashMap::new();
    let mut qr = csv::Reader::from_path(&args.query_log)
        .unwrap_or_else(|e| panic!("open {}: {e}", args.query_log));
    for rec in qr.records() {
        let r = rec.expect("query_log row");
        let get = |i: usize| r.get(i).unwrap_or("").to_string();
        ql.insert(get(0), [get(1), get(2), get(3), get(4), get(5)]);
    }

    let mut cr = csv::Reader::from_path(&args.client)
        .unwrap_or_else(|e| panic!("open {}: {e}", args.client));
    let mut w = csv::Writer::from_path(&args.out).expect("create results.csv");
    w.write_record([
        "ts_ms",
        "round",
        "vu",
        "request_id",
        "endpoint",
        "method",
        "http_status",
        "err_class",
        "duration_ms",
        "ttfb_ms",
        "read_rows",
        "read_bytes",
        "ch_queries",
        "ch_duration_ms",
        "memory_max",
        "url",
    ])
    .expect("write header");

    let blank: [String; 5] = std::array::from_fn(|_| String::new());
    let (mut matched, mut total) = (0u64, 0u64);
    for rec in cr.records() {
        let c = rec.expect("client row");
        total += 1;
        let rid = c.get(3).unwrap_or("");
        let q = match ql.get(rid) {
            Some(q) => {
                matched += 1;
                q
            }
            None => &blank,
        };
        let g = |i: usize| c.get(i).unwrap_or("");
        // client cols ts_ms..ttfb (0..=9), then the 5 CH cols, then url (10) last.
        w.write_record([
            g(0),
            g(1),
            g(2),
            g(3),
            g(4),
            g(5),
            g(6),
            g(7),
            g(8),
            g(9),
            q[0].as_str(),
            q[1].as_str(),
            q[2].as_str(),
            q[3].as_str(),
            q[4].as_str(),
            g(10),
        ])
        .expect("write row");
    }
    w.flush().expect("flush results.csv");
    eprintln!(
        "joined {matched}/{total} client requests matched query_log ({} query_log rows) -> {}",
        ql.len(),
        args.out
    );
}
