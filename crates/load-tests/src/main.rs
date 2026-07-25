//! HTTP load-test harness for the Soroban Block Explorer API (task 0338, Step 4).
//!
//! Two drivers, picked by which flag you pass:
//!
//! - `--rps` — **open model** (task 0357). Poisson arrivals on a schedule fixed
//!   in advance, spawn-and-forget. The rate is an INPUT, so this is the mode
//!   that expresses an AC4 "N req/month" target (`rps = N / 2_592_000`), and it
//!   does not suffer coordinated omission: when the backend slows, the client
//!   keeps arriving on schedule instead of quietly backing off.
//! - `--vus` — **closed model**. N users sweeping every endpoint with no
//!   think-time. The rate is an OUTPUT (whatever the server allows), so it
//!   cannot express a req/month target. Saturation / knee-finding only: it is
//!   EXPECTED to shed load (429 / 5xx; CH "Code 202 Too many simultaneous
//!   queries" surfaces as an HTTP 5xx) once a backend ceiling is hit.
//!
//! Both drivers hit the same endpoint catalogue and emit the same rows. Each
//! request carries a unique `X-Request-Id` of the form `<endpoint-code>-<hex>`
//! so the server stamps `system.query_log.log_comment` with it (B2
//! correlation): the endpoint code is the first segment, so query_log can be
//! grouped per endpoint with no client-side join, and the full id still joins
//! back per request. See README.md.
//!
//! Outputs (in `--out-dir`):
//!   - `client.csv`         one row per request (for the per-request query_log join)
//!   - `client_summary.csv` per-endpoint p50/p90/p95/p99/max + error rate
//!
//! The per-request CSV is written by a dedicated OS thread (file IO off the
//! async runtime); workers feed it over an mpsc channel.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use hdrhistogram::Histogram;
use reqwest::header::{HeaderMap, HeaderValue};

#[derive(Parser)]
#[command(about = "Load-test the Soroban Block Explorer API (task 0338)")]
struct Args {
    /// API base URL (through Cloudflare in prod).
    #[arg(
        long,
        env = "BASE_URL",
        default_value = "https://api-sorobanscan.rumblefishdev.com/v1"
    )]
    base_url: String,
    /// Paid-tier `x-api-key` — REQUIRED in prod (bypasses Turnstile/JWT). Must be in the server `API_KEYS`.
    /// `hide_env_values` so `--help` / usage never echoes the live secret.
    #[arg(long, env = "API_KEY", default_value = "", hide_env_values = true)]
    api_key: String,
    /// Fallback `x-edge-secret` — only needed if Cloudflare does not inject it.
    #[arg(long, env = "EDGE_SECRET", default_value = "", hide_env_values = true)]
    edge_secret: String,
    /// Concurrent virtual users (held alive for the whole window). Closed model:
    /// the rate is an OUTPUT (whatever the server allows), so this cannot express
    /// a req/month target — use `--rps` for that. Kept for saturation runs.
    #[arg(long, env = "VUS", default_value_t = 1000)]
    vus: usize,
    /// Open-model arrival rate (req/s), Poisson. Overrides `--vus`.
    ///
    /// This is the mode that expresses an "N requests/month" target:
    /// `rps = N / 2_592_000` (30d). 1M/mo = 0.386, 10M/mo = 3.858, 50M/mo = 19.29.
    ///
    /// Arrivals are spawn-and-forget on a schedule fixed in advance, so the offered
    /// load does NOT back off when the server slows — that back-off is coordinated
    /// omission, and it is why a closed-model p95 flatters a struggling backend.
    #[arg(long, env = "RPS")]
    rps: Option<f64>,
    /// Test duration, e.g. `1h`, `15m`, `30s`.
    #[arg(long, env = "DURATION", default_value = "1h")]
    duration: String,
    /// Ids harvested per list endpoint in setup(). Values >100 paginate via
    /// cursor (API caps a page at 100), e.g. `HARVEST=10000` for wide coverage.
    #[arg(long, env = "HARVEST", default_value_t = 100)]
    harvest: usize,
    /// Base output dir. Each run writes a timestamped subdir under it (the
    /// subdir name is the run's UTC start — Rust-generated, no name to pass),
    /// holding `client.csv` + `client_summary.csv`. Defaults to the crate's
    /// `out/` (gitignored), resolved absolutely so it works from any cwd.
    #[arg(long, env = "OUT_DIR", default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/out"))]
    out_dir: String,
}

/// Real ids harvested from list endpoints, randomised per request.
#[derive(Default)]
struct Harvest {
    tx_hashes: Vec<String>,
    accounts: Vec<String>,
    contracts: Vec<String>,
    assets: Vec<String>,
    ledgers: Vec<String>,
    pools: Vec<String>,
    nfts: Vec<(String, String)>,
}

/// One endpoint in the sweep. `build` returns the relative path with a random
/// id, or `None` when its harvest pool is empty (endpoint skipped this round).
struct Ep {
    code: &'static str,
    heavy: bool,
    build: fn(&Harvest) -> Option<String>,
}

/// One completed request, handed to the writer thread.
struct Sample {
    ts_ms: u128,
    round: u64,
    vu: usize,
    rid: String,
    code: &'static str,
    status: u16,
    class: &'static str,
    dur_ms: u64,
    ttfb_ms: u64,
    url: String,
}

fn pick(v: &[String]) -> Option<&String> {
    if v.is_empty() {
        None
    } else {
        Some(&v[fastrand::usize(..v.len())])
    }
}

fn pick_nft(v: &[(String, String)]) -> Option<&(String, String)> {
    if v.is_empty() {
        None
    } else {
        Some(&v[fastrand::usize(..v.len())])
    }
}

fn rand_prefix() -> char {
    (b'a' + fastrand::u8(..26)) as char
}

/// `<code>-<128-bit hex>` — sanitiser-safe ([a-z0-9-]); the code prefix lets
/// query_log group per endpoint, the full string joins per request.
fn request_id(code: &str) -> String {
    format!(
        "{code}-{:016x}{:016x}",
        fastrand::u64(..),
        fastrand::u64(..)
    )
}

fn err_class(status: u16) -> &'static str {
    match status {
        200 => "ok",
        // Non-200 success is anomalous for these data GETs (a 204, or an edge
        // shed/challenge served as 2xx) — counted as non-ok so it can't silently
        // pass. The CH "too many simultaneous queries" shed is an HTTP 5xx, below.
        201..=299 => "2xx",
        0 => "conn", // client-side timeout / connection reset / body-read failure
        401 => "auth",
        403 => "edge",
        429 => "throttle",
        504 => "timeout",
        500..=599 => "5xx",
        400..=499 => "4xx",
        300..=399 => "3xx", // reqwest follows redirects, so this is anomalous
        _ => "other",
    }
}

/// The full endpoint catalogue swept each iteration. Non-capturing closures
/// coerce to `fn` pointers. `heavy` flags the archive-fetch / non-PK-scan paths.
fn endpoints() -> Vec<Ep> {
    vec![
        // Lists (no id needed).
        Ep {
            code: "txlist",
            heavy: false,
            build: |_| Some("/transactions?limit=20".into()),
        },
        Ep {
            code: "acclist",
            heavy: false,
            build: |_| Some("/accounts?limit=20".into()),
        },
        Ep {
            code: "ctrlist",
            heavy: false,
            build: |_| Some("/contracts?limit=20".into()),
        },
        Ep {
            code: "astlist",
            heavy: false,
            build: |_| Some("/assets?limit=20".into()),
        },
        Ep {
            code: "ldglist",
            heavy: false,
            build: |_| Some("/ledgers?limit=20".into()),
        },
        Ep {
            code: "lplist",
            heavy: false,
            build: |_| Some("/liquidity-pools?limit=20".into()),
        },
        Ep {
            code: "nftlist",
            heavy: false,
            build: |_| Some("/nfts?limit=20".into()),
        },
        Ep {
            code: "netstats",
            heavy: false,
            build: |_| Some("/network/stats".into()),
        },
        // Details + aggregations (random harvested id).
        Ep {
            code: "txdetail",
            heavy: true,
            build: |h| pick(&h.tx_hashes).map(|x| format!("/transactions/{x}")),
        },
        Ep {
            code: "accdetail",
            heavy: false,
            build: |h| pick(&h.accounts).map(|x| format!("/accounts/{x}")),
        },
        Ep {
            code: "acctxs",
            heavy: false,
            build: |h| pick(&h.accounts).map(|x| format!("/accounts/{x}/transactions?limit=20")),
        },
        Ep {
            code: "ctrdetail",
            heavy: false,
            build: |h| pick(&h.contracts).map(|x| format!("/contracts/{x}")),
        },
        Ep {
            code: "ctriface",
            heavy: false,
            build: |h| pick(&h.contracts).map(|x| format!("/contracts/{x}/interface")),
        },
        Ep {
            code: "ctrinvoc",
            heavy: false,
            build: |h| pick(&h.contracts).map(|x| format!("/contracts/{x}/invocations?limit=20")),
        },
        Ep {
            code: "ctrevents",
            heavy: true,
            build: |h| pick(&h.contracts).map(|x| format!("/contracts/{x}/events?limit=20")),
        },
        Ep {
            code: "astdetail",
            heavy: true,
            build: |h| pick(&h.assets).map(|x| format!("/assets/{x}")),
        },
        Ep {
            code: "asttxs",
            heavy: false,
            build: |h| pick(&h.assets).map(|x| format!("/assets/{x}/transactions?limit=20")),
        },
        Ep {
            code: "ldgdetail",
            heavy: false,
            build: |h| pick(&h.ledgers).map(|x| format!("/ledgers/{x}")),
        },
        Ep {
            code: "lpdetail",
            heavy: false,
            build: |h| pick(&h.pools).map(|x| format!("/liquidity-pools/{x}")),
        },
        Ep {
            code: "lpchart",
            heavy: true,
            build: |h| pick(&h.pools).map(|x| format!("/liquidity-pools/{x}/chart?interval=1h")),
        },
        Ep {
            code: "lptxs",
            heavy: false,
            build: |h| {
                pick(&h.pools).map(|x| format!("/liquidity-pools/{x}/transactions?limit=20"))
            },
        },
        Ep {
            code: "lpparts",
            heavy: false,
            build: |h| pick(&h.pools).map(|x| format!("/liquidity-pools/{x}/participants")),
        },
        Ep {
            code: "nftdetail",
            heavy: false,
            build: |h| pick_nft(&h.nfts).map(|(c, t)| format!("/nfts/{c}/{t}")),
        },
        // Filtered lists — non-PK scans (read_rows-heavy).
        Ep {
            code: "txfilter",
            heavy: true,
            build: |h| {
                pick(&h.contracts)
                    .map(|x| format!("/transactions?filter[contract_id]={x}&limit=20"))
            },
        },
        Ep {
            code: "ctrfilter",
            heavy: true,
            build: |_| Some(format!("/contracts?filter[q]={}&limit=20", rand_prefix())),
        },
        Ep {
            code: "search",
            heavy: true,
            build: |_| Some(format!("/search?q={}", rand_prefix())),
        },
    ]
}

/// Exp(`rate`) inter-arrival gap, seconds — the Poisson process a population of
/// independent users produces. Mean = `1/rate`. `max()` guards `ln(0) = -inf`.
fn arrival_gap(rate: f64) -> f64 {
    -fastrand::f64().max(f64::MIN_POSITIVE).ln() / rate
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Format epoch seconds as UTC `YYYY-MM-DD HH:MM:SS` — ready to paste 1:1 into
/// a ClickHouse `--param_start`/`--param_end` DateTime (civil-from-days, no dep).
fn fmt_utc(secs: u64) -> String {
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    let mut z = (secs / 86_400) as i64 + 719_468; // days since 1970-01-01, shifted
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    z -= era * 146_097;
    let yoe = (z - z / 1_460 + z / 36_524 - z / 146_096) / 365;
    let doy = z - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = era * 400 + yoe + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// Filesystem-safe variant of [`fmt_utc`] for a run's directory name.
fn dir_stamp(secs: u64) -> String {
    fmt_utc(secs).replace(' ', "T").replace(':', "-") + "Z"
}

fn parse_duration(s: &str) -> Duration {
    let s = s.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: u64 = s[..split].parse().unwrap_or(3600);
    match s[split..].trim() {
        "h" => Duration::from_secs(n * 3600),
        "m" => Duration::from_secs(n * 60),
        _ => Duration::from_secs(n), // "s" or bare number
    }
}

/// A JSON value's string form usable as a path segment: the inner string for a
/// JSON string, the literal for a number. `None` for other shapes.
fn id_str(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        Some(s.to_string())
    } else if v.is_number() {
        Some(v.to_string())
    } else {
        None
    }
}

async fn fetch_json(client: &reqwest::Client, url: &str) -> serde_json::Value {
    match client
        .get(url)
        .header("x-request-id", request_id("harvest"))
        .send()
        .await
    {
        Ok(r) => r
            .text()
            .await
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

fn data_items(v: &serde_json::Value) -> &[serde_json::Value] {
    v.get("data")
        .and_then(|d| d.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Paginate a list endpoint (`?limit=100&cursor=`) collecting up to `target`
/// extracted ids, or until the pages run out. The API caps `limit` at 100
/// (MAX_LIMIT), so harvesting more than 100 ids requires walking cursors — which
/// also exercises the deep-cursor decode path with real variety.
async fn harvest<T, F>(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    target: usize,
    extract: F,
) -> Vec<T>
where
    F: Fn(&serde_json::Value) -> Option<T>,
{
    const PAGE: usize = 100; // == API MAX_LIMIT
    // Hard page cap: enough to reach `target` even if most rows fail to extract
    // (×3 slack), so a drifted field name or a self-referential cursor can NEVER
    // crawl the whole prod table during setup. Without it, an empty `extract`
    // never grows `out` and the loop would walk every page in the table.
    let max_pages = target.div_ceil(PAGE).saturating_mul(3).max(5);
    let mut out: Vec<T> = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..max_pages {
        if out.len() >= target {
            break;
        }
        let url = match &cursor {
            Some(c) => format!("{base}{path}?limit={PAGE}&cursor={c}"),
            None => format!("{base}{path}?limit={PAGE}"),
        };
        let v = fetch_json(client, &url).await;
        let items = data_items(&v);
        if items.is_empty() {
            break;
        }
        out.extend(items.iter().filter_map(&extract));
        match v
            .get("page")
            .and_then(|p| p.get("next_cursor"))
            .and_then(|c| c.as_str())
        {
            // Defensive: stop if the cursor does not advance (would loop forever).
            Some(c) if Some(c) != cursor.as_deref() => cursor = Some(c.to_string()),
            _ => break,
        }
    }
    out.truncate(target);
    out
}

async fn setup(client: &reqwest::Client, base: &str, n: usize) -> Harvest {
    // The 7 entity harvests are independent → paginate them concurrently.
    // LP/NFT field names are defensive — first candidate that resolves.
    let (tx_hashes, accounts, contracts, assets, ledgers, pools, nfts) = tokio::join!(
        harvest(client, base, "/transactions", n, |x| x
            .get("hash")?
            .as_str()
            .map(String::from)),
        harvest(client, base, "/accounts", n, |x| x
            .get("account_id")?
            .as_str()
            .map(String::from)),
        harvest(client, base, "/contracts", n, |x| x
            .get("contract_id")?
            .as_str()
            .map(String::from)),
        harvest(client, base, "/assets", n, |x| x
            .get("id")?
            .as_str()
            .map(String::from)),
        harvest(client, base, "/ledgers", n, |x| x
            .get("sequence")
            .and_then(id_str)),
        harvest(client, base, "/liquidity-pools", n, |x| x
            .get("pool_id")
            .or_else(|| x.get("address"))
            .or_else(|| x.get("id"))
            .and_then(id_str)),
        harvest(client, base, "/nfts", n, |x| {
            let c = x.get("contract_id")?.as_str()?.to_string();
            let t = x.get("token_id").and_then(id_str)?;
            Some((c, t))
        }),
    );
    Harvest {
        tx_hashes,
        accounts,
        contracts,
        assets,
        ledgers,
        pools,
        nfts,
    }
}

/// Per-endpoint accumulator for the summary CSV.
struct EpAgg {
    reqs: u64,
    errs: u64,
    hist: Histogram<u64>,
}

impl Default for EpAgg {
    fn default() -> Self {
        // 3 significant figures, auto-resizing — no fixed max to clip slow tails.
        Self {
            reqs: 0,
            errs: 0,
            hist: Histogram::<u64>::new(3).expect("histogram"),
        }
    }
}

/// Dedicated writer thread: drains samples, appends `client.csv`, accumulates
/// per-endpoint stats, and writes `client_summary.csv` when the channel closes.
fn writer_thread(rx: std::sync::mpsc::Receiver<Sample>, out_dir: String) {
    let f = std::fs::File::create(format!("{out_dir}/client.csv")).expect("create client.csv");
    let mut w = BufWriter::new(f);
    writeln!(
        w,
        "ts_ms,round,vu,request_id,endpoint,method,http_status,err_class,duration_ms,ttfb_ms,url"
    )
    .ok();

    let mut aggs: HashMap<&'static str, EpAgg> = HashMap::new();
    let mut n: u64 = 0;
    while let Ok(s) = rx.recv() {
        // `url` is the last column and quoted — a query string can in principle
        // carry a comma, which unquoted would shift columns for a naive parser.
        writeln!(
            w,
            "{},{},{},{},{},GET,{},{},{},{},\"{}\"",
            s.ts_ms, s.round, s.vu, s.rid, s.code, s.status, s.class, s.dur_ms, s.ttfb_ms, s.url
        )
        .ok();
        let a = aggs.entry(s.code).or_default();
        a.reqs += 1;
        if s.class != "ok" {
            a.errs += 1;
        }
        a.hist.record(s.dur_ms.max(1)).ok();
        n += 1;
        if n.is_multiple_of(2000) {
            w.flush().ok();
        }
    }
    w.flush().ok();
    write_summary(&aggs, &out_dir);
}

fn write_summary(aggs: &HashMap<&'static str, EpAgg>, out_dir: &str) {
    let mut s = String::from(
        "endpoint,heavy,requests,errors,err_rate_pct,p50_ms,p90_ms,p95_ms,p99_ms,max_ms\n",
    );
    for ep in endpoints() {
        let Some(a) = aggs.get(ep.code) else { continue };
        let rate = if a.reqs > 0 {
            a.errs as f64 / a.reqs as f64 * 100.0
        } else {
            0.0
        };
        s.push_str(&format!(
            "{},{},{},{},{rate:.2},{},{},{},{},{}\n",
            ep.code,
            if ep.heavy { "Y" } else { "" },
            a.reqs,
            a.errs,
            a.hist.value_at_quantile(0.5),
            a.hist.value_at_quantile(0.9),
            a.hist.value_at_quantile(0.95),
            a.hist.value_at_quantile(0.99),
            a.hist.max(),
        ));
    }
    let path = format!("{out_dir}/client_summary.csv");
    std::fs::write(&path, &s).expect("write client_summary.csv");
    eprintln!("\n{s}\nwrote {path}");
}

/// Issue one request against `ep` and return its sample. `None` when the
/// endpoint's harvest pool is empty (nothing to ask for). Shared by both
/// drivers; the caller owns the counters + the writer channel, so no `&Sender`
/// is ever held across an await (that would make the future `!Send`).
async fn do_request(
    client: &reqwest::Client,
    base: &str,
    ep: &Ep,
    harvest: &Harvest,
    round: u64,
    vu: usize,
) -> Option<Sample> {
    let rel = (ep.build)(harvest)?;
    let url = format!("{base}{rel}");
    let rid = request_id(ep.code);
    let t0 = Instant::now();
    let (status, ttfb_ms, dur_ms) = match client.get(&url).header("x-request-id", &rid).send().await
    {
        Ok(resp) => {
            let ttfb = t0.elapsed().as_millis() as u64;
            let st = resp.status().as_u16();
            // Drain the body → total time + connection reuse. A body-read
            // error (e.g. the 35s timeout fires mid-body) must NOT be logged
            // as a successful `st` — fold it into status 0 ("conn") so a hung
            // response surfaces instead of hiding as a slow 200.
            let st = if resp.bytes().await.is_ok() { st } else { 0 };
            (st, ttfb, t0.elapsed().as_millis() as u64)
        }
        Err(_) => {
            let e = t0.elapsed().as_millis() as u64;
            (0u16, e, e)
        }
    };
    Some(Sample {
        ts_ms: now_ms(),
        round,
        vu,
        rid,
        code: ep.code,
        status,
        class: err_class(status),
        dur_ms,
        ttfb_ms,
        url,
    })
}

/// Book a finished sample: counters + hand to the writer thread.
fn record(s: Sample, tx: &std::sync::mpsc::Sender<Sample>, total: &AtomicU64, errors: &AtomicU64) {
    total.fetch_add(1, Ordering::Relaxed);
    if s.class != "ok" {
        errors.fetch_add(1, Ordering::Relaxed);
    }
    tx.send(s).ok();
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let dur = parse_duration(&args.duration);

    // Each run gets its own timestamped subdir under the base out-dir, named by
    // the run's UTC start (no name to pass). Created up front so an unwritable
    // path fails fast, not only when the writer thread panics at the end.
    let run_start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let run_dir = format!("{}/{}", args.out_dir, dir_stamp(run_start));
    std::fs::create_dir_all(&run_dir).expect("create run out dir");

    let mut headers = HeaderMap::new();
    if !args.api_key.is_empty() {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&args.api_key).expect("invalid API_KEY header"),
        );
    }
    if !args.edge_secret.is_empty() {
        headers.insert(
            "x-edge-secret",
            HeaderValue::from_str(&args.edge_secret).expect("invalid EDGE_SECRET header"),
        );
    }
    let client = reqwest::Client::builder()
        .default_headers(headers)
        // reqwest sends NO User-Agent by default; AWS WAF CommonRuleSet's
        // `NoUserAgent_HEADER` rule then 403s every request. Set one explicitly.
        .user_agent(concat!("soroban-load-tests/", env!("CARGO_PKG_VERSION")))
        .pool_max_idle_per_host(args.vus)
        // Above the CH 30s per-query cap so a hung request surfaces, not blocks forever.
        .timeout(Duration::from_secs(35))
        .build()
        .expect("build reqwest client");

    eprintln!(
        "[setup] harvesting up to {} ids per list endpoint...",
        args.harvest
    );
    let harvest = Arc::new(setup(&client, &args.base_url, args.harvest).await);
    eprintln!(
        "[setup] tx={} acc={} ctr={} ast={} ldg={} lp={} nft={}",
        harvest.tx_hashes.len(),
        harvest.accounts.len(),
        harvest.contracts.len(),
        harvest.assets.len(),
        harvest.ledgers.len(),
        harvest.pools.len(),
        harvest.nfts.len(),
    );
    // An entity that harvested 0 ids silently drops all its detail endpoints from
    // the run — warn loudly so a drifted field name / empty table is noticed.
    for (name, len) in [
        ("transactions", harvest.tx_hashes.len()),
        ("accounts", harvest.accounts.len()),
        ("contracts", harvest.contracts.len()),
        ("assets", harvest.assets.len()),
        ("ledgers", harvest.ledgers.len()),
        ("liquidity-pools", harvest.pools.len()),
        ("nfts", harvest.nfts.len()),
    ] {
        if len == 0 {
            eprintln!("[setup] WARN: 0 ids for {name} — its detail endpoints will be skipped");
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<Sample>();
    let writer = std::thread::spawn({
        let out_dir = run_dir.clone();
        move || writer_thread(rx, out_dir)
    });

    let total = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + dur;

    // Live progress every 10s.
    tokio::spawn({
        let (total, errors) = (total.clone(), errors.clone());
        async move {
            let mut last = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let t = total.load(Ordering::Relaxed);
                let e = errors.load(Ordering::Relaxed);
                let pct = if t > 0 {
                    e as f64 / t as f64 * 100.0
                } else {
                    0.0
                };
                eprintln!(
                    "[+] reqs={t} rps~{} errors={e} ({pct:.2}%)",
                    (t - last) / 10
                );
                last = t;
                if Instant::now() >= deadline {
                    break;
                }
            }
        }
    });

    let eps = Arc::new(endpoints());
    let mut workers: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    match args.rps {
        // Open model — arrivals on a Poisson schedule fixed in advance, independent
        // of how fast the server answers. The only mode that can express a
        // req/month target, and the only one free of coordinated omission.
        // `--vus` is ignored here (it still sizes the connection pool).
        Some(rps) => {
            eprintln!(
                "[run] open model: {rps} rps Poisson (~{:.1}M req/month), until {:?} from now",
                rps * 2_592_000.0 / 1e6,
                dur
            );
            let mut next = Instant::now();
            let mut n = 0u64;
            loop {
                // Exp(rate) inter-arrival. `next` accumulates ABSOLUTELY — a late tick
                // must not push the following arrival out, or the offered load would
                // quietly back off exactly when the backend is struggling.
                next += Duration::from_secs_f64(arrival_gap(rps));
                if next >= deadline {
                    break;
                }
                tokio::time::sleep_until(next.into()).await;
                let (client, harvest, eps, tx, base, total, errors) = (
                    client.clone(),
                    harvest.clone(),
                    eps.clone(),
                    tx.clone(),
                    args.base_url.clone(),
                    total.clone(),
                    errors.clone(),
                );
                let idx = fastrand::usize(..eps.len());
                // Spawn-and-forget: a slow response must never delay the next arrival.
                workers.push(tokio::spawn(async move {
                    if let Some(s) = do_request(&client, &base, &eps[idx], &harvest, n, 0).await {
                        record(s, &tx, &total, &errors);
                    }
                }));
                n += 1;
            }
        }
        // Closed model — `--vus` users sweeping every endpoint with no think-time.
        // Saturation / knee-finding only: the rate is whatever the server allows.
        None => {
            eprintln!(
                "[run] closed model: {} VUs, no think-time, until {:?} from now",
                args.vus, dur
            );
            for vu in 0..args.vus {
                let (client, harvest, eps, tx, base, total, errors) = (
                    client.clone(),
                    harvest.clone(),
                    eps.clone(),
                    tx.clone(),
                    args.base_url.clone(),
                    total.clone(),
                    errors.clone(),
                );
                workers.push(tokio::spawn(async move {
                    let mut round = 0u64;
                    while Instant::now() < deadline {
                        for ep in eps.iter() {
                            if Instant::now() >= deadline {
                                break;
                            }
                            if let Some(s) =
                                do_request(&client, &base, ep, &harvest, round, vu).await
                            {
                                record(s, &tx, &total, &errors);
                            }
                        }
                        round += 1;
                    }
                }));
            }
        }
    }
    drop(tx); // workers hold clones; this lets the writer thread finish once they exit

    for w in workers {
        w.await.ok();
    }
    writer.join().expect("writer thread");
    let run_end = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(run_start);
    eprintln!(
        "[done] total={} errors={}",
        total.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed)
    );
    eprintln!("[out] {run_dir}");
    // Ready-to-paste query_log window. End is padded +90s so requests still
    // finishing (up to the 30s cap) and the async query_log flush land inside it.
    eprintln!(
        "[query_log window] --param_start='{}' --param_end='{}'",
        fmt_utc(run_start),
        fmt_utc(run_end + 90),
    );
}

#[cfg(test)]
mod tests {
    use super::arrival_gap;

    /// The whole open model rests on this one line: a dropped `-` or a `* rate`
    /// still produces plausible-looking gaps, but silently runs the wrong load.
    #[test]
    fn arrival_gap_mean_is_one_over_rate() {
        for rate in [0.386_f64, 3.858, 19.29] {
            let n = 200_000;
            let mean = (0..n).map(|_| arrival_gap(rate)).sum::<f64>() / f64::from(n);
            let want = 1.0 / rate;
            // Exp has sd == mean, so the sample mean's sd is mean/sqrt(n) ≈ 0.22%;
            // 2% is ~9 sigma — tight enough to catch real bugs, never flaky.
            assert!(
                (mean - want).abs() < want * 0.02,
                "rate {rate}: mean gap {mean} != ~{want}"
            );
        }
    }
}
