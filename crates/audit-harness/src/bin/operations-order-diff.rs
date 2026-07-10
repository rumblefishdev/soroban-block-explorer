//! Phase-0 audit for task 0192 — DB op-row order vs on-chain apply order.
//!
//! For a sample of multi-op transactions, compares the order of
//! `operations_appearances` rows (`ORDER BY oa.id`, the order surfaced by
//! endpoint 03 Statement C) against the on-chain apply order produced by the
//! XDR walk in `tx.operations[]`. Divergence indicates that the indexer's
//! HashMap-keyed aggregation in `crates/indexer/src/handler/persist/staging.rs`
//! is producing rows in alphabetic-by-asset_code order rather than apply order.
//!
//! Output is the % rows affected; the gate for whether the task-0192 fix
//! requires a backfill of historical rows or a forward-only persist change.
//!
//! ## Independence note
//!
//! The Phase 2c convention (see `archive-diff.rs`) is to re-parse XDR
//! independently of `crates/xdr-parser`. This bin is the deliberate
//! exception: the bug under audit lives in `staging.rs` (downstream of the
//! parser), so reusing `xdr_parser::extract_envelopes` /
//! `extract_operations` is non-circular and avoids re-implementing the
//! tx_set ↔ tx_processing hash alignment (which itself was a real bug
//! per the envelope.rs commentary).
//!
//! ## Sample scope
//!
//! Soroban (`has_soroban = true`) transactions are skipped — they are
//! single-op by protocol (one `InvokeHostFunction`) so ordering does not
//! apply, and `contract_id` projection between DB JOIN-strkey and XDR JSON
//! details is not normalized in this bin.
//!
//! ## Usage
//!
//!   DATABASE_URL=postgres://... cargo run -p audit-harness --bin operations-order-diff -- \
//!       --sample 200 --concurrency 4

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use clap::Parser;
use domain::OperationType;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use stellar_xdr::{LedgerCloseMeta, LedgerCloseMetaBatch, Limits, ReadXdr, TransactionMeta};
use tokio::sync::{Mutex, OnceCell, Semaphore};
use xdr_parser::ExtractedOperation;

const PARTITION_SIZE: u32 = 64_000;
const ARCHIVE_BASE: &str =
    "https://aws-public-blockchain.s3.amazonaws.com/v1.1/stellar/ledgers/pubnet";
const DEFAULT_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

#[derive(Parser)]
#[command(name = "operations-order-diff", about)]
struct Cli {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[arg(long, env = "ARCHIVE_URL", default_value = ARCHIVE_BASE)]
    archive_url: String,

    #[arg(long, env = "STELLAR_NETWORK_PASSPHRASE", default_value = DEFAULT_PASSPHRASE)]
    network_passphrase: String,

    /// Number of multi-op tx rows to sample. Each sample = 1 S3 GET (cached
    /// per ledger) + XDR parse + DB query.
    #[arg(long, default_value_t = 200)]
    sample: usize,

    /// Skip transactions with fewer operations than this. Single-op txs
    /// cannot exhibit the ordering bug.
    #[arg(long, default_value_t = 2)]
    min_ops: i16,

    /// Concurrent in-flight ledger fetches. The expensive step is XDR
    /// decompression + parse, which saturates ~4 cores in parallel.
    #[arg(long, default_value_t = 4)]
    concurrency: usize,

    /// Deterministic SQL `setseed` value in [-1, 1]. Same seed against the
    /// same DB → same sample, so a re-run produces an identical Diverge
    /// count.
    #[arg(long, default_value_t = 0.42)]
    seed: f64,

    /// Optional path to dump the full report as JSON (in addition to the
    /// markdown summary printed to stdout).
    #[arg(long)]
    report_json: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let db = PgPoolOptions::new()
        .max_connections(4)
        .connect(&cli.database_url)
        .await?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("soroban-block-explorer/audit-harness/0.1")
        .build()?;

    let network_id: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(cli.network_passphrase.as_bytes());
        h.finalize().into()
    };

    let samples = sample_transactions(&db, cli.seed, cli.min_ops, cli.sample).await?;
    if samples.is_empty() {
        eprintln!("no multi-op non-Soroban transactions matched the sample criteria");
        return Ok(());
    }
    tracing::info!(count = samples.len(), "sampled transactions");

    let report = run_diff(
        Arc::new(db),
        http,
        Arc::new(cli.archive_url),
        Arc::new(network_id),
        samples,
        cli.concurrency,
    )
    .await?;

    print_report(&report);
    if let Some(path) = &cli.report_json {
        let json = serde_json::to_vec_pretty(&report)?;
        std::fs::write(path, json)?;
        eprintln!("report JSON written to {}", path.display());
    }

    // Divergence is the expected signal of this audit — exit 0 regardless so
    // that running this in a script does not require swallowing a non-zero
    // exit. The numeric `diverged` count + `report_json` is what the caller
    // reads.
    Ok(())
}

#[derive(Debug, Clone)]
struct SampledTx {
    hash_hex: String,
    tx_id: i64,
    ledger_sequence: u32,
    created_at: DateTime<Utc>,
    operation_count: i16,
}

async fn sample_transactions(
    db: &sqlx::PgPool,
    seed: f64,
    min_ops: i16,
    sample: usize,
) -> Result<Vec<SampledTx>, sqlx::Error> {
    // Bind setseed + sample query on the same connection so the random()
    // sequence is reproducible. setseed accepts [-1, 1]; clamp defensively.
    let seed = seed.clamp(-1.0, 1.0);
    let mut tx = db.begin().await?;
    sqlx::query("SELECT setseed($1)")
        .bind(seed)
        .execute(&mut *tx)
        .await?;

    // `parse_error = false` skips degraded rows (no envelope_xdr available);
    // `has_soroban = false` keeps the comparison on classic multi-op tx where
    // the ordering bug actually surfaces.
    let rows = sqlx::query(
        "SELECT encode(t.hash, 'hex')  AS hash_hex,
                t.id                   AS tx_id,
                t.ledger_sequence,
                t.created_at,
                t.operation_count
           FROM transactions t
          WHERE t.operation_count >= $1
            AND t.has_soroban = false
            AND t.parse_error = false
          ORDER BY random()
          LIMIT $2",
    )
    .bind(min_ops)
    .bind(sample as i64)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let ledger_sequence: i64 = r.get("ledger_sequence");
            SampledTx {
                hash_hex: r.get("hash_hex"),
                tx_id: r.get("tx_id"),
                ledger_sequence: u32::try_from(ledger_sequence).expect("ledger_sequence fits u32"),
                created_at: r.get("created_at"),
                operation_count: r.get("operation_count"),
            }
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OpIdentity {
    op_type: i16,
    source: Option<String>,
    destination: Option<String>,
    contract_id: Option<String>,
    asset_code: Option<String>,
    asset_issuer: Option<String>,
    pool_id_hex: Option<String>,
}

impl OpIdentity {
    fn render(&self) -> String {
        let optype = OperationType::try_from(self.op_type)
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|_| format!("type={}", self.op_type));
        format!(
            "{}{}{}{}{}{}{}",
            optype,
            self.source
                .as_deref()
                .map(|s| format!(" src={}", short(s)))
                .unwrap_or_default(),
            self.destination
                .as_deref()
                .map(|s| format!(" dst={}", short(s)))
                .unwrap_or_default(),
            self.asset_code
                .as_deref()
                .map(|s| format!(" code={s}"))
                .unwrap_or_default(),
            self.asset_issuer
                .as_deref()
                .map(|s| format!(" iss={}", short(s)))
                .unwrap_or_default(),
            self.contract_id
                .as_deref()
                .map(|s| format!(" cid={}", short(s)))
                .unwrap_or_default(),
            self.pool_id_hex
                .as_deref()
                .map(|s| format!(" pool={}", short(s)))
                .unwrap_or_default(),
        )
    }
}

fn short(s: &str) -> String {
    if s.len() > 12 {
        format!("{}…", &s[..12])
    } else {
        s.to_string()
    }
}

#[derive(Debug, Clone)]
struct TxComparison {
    tx_hash: String,
    operation_count: i16,
    db_order: Vec<OpIdentity>,
    xdr_order: Vec<OpIdentity>,
}

impl TxComparison {
    fn outcome(&self) -> Outcome {
        if self.db_order == self.xdr_order {
            Outcome::Match
        } else {
            let first = self
                .db_order
                .iter()
                .zip(self.xdr_order.iter())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| self.db_order.len().min(self.xdr_order.len()));
            Outcome::Diverge { first_div: first }
        }
    }

    fn dominant_op_type(&self) -> String {
        let mut counts: HashMap<i16, usize> = HashMap::new();
        for o in &self.xdr_order {
            *counts.entry(o.op_type).or_insert(0) += 1;
        }
        let dominant = counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(t, _)| t)
            .unwrap_or(0);
        OperationType::try_from(dominant)
            .map(|t| format!("{t:?}"))
            .unwrap_or_else(|_| format!("type={dominant}"))
    }
}

#[derive(Debug, Clone, Copy)]
enum Outcome {
    Match,
    Diverge { first_div: usize },
}

async fn run_diff(
    db: Arc<sqlx::PgPool>,
    http: reqwest::Client,
    archive_url: Arc<String>,
    network_id: Arc<[u8; 32]>,
    samples: Vec<SampledTx>,
    concurrency: usize,
) -> Result<Report, Box<dyn std::error::Error + Send + Sync>> {
    // Per-ledger singleflight: the inner `OnceCell` is a write-once slot
    // shared across all tasks that ask for the same ledger sequence —
    // exactly one of them runs the S3 GET + zstd decode + XDR parse;
    // the rest await the same future and read its result. Without this,
    // concurrent tasks sampling distinct txs from the same ledger would
    // each issue a redundant fetch + parse before any of them got to
    // populate the cache.
    type LedgerCell = OnceCell<Arc<LedgerCloseMeta>>;
    let cache: Arc<Mutex<HashMap<u32, Arc<LedgerCell>>>> = Arc::new(Mutex::new(HashMap::new()));
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(samples.len());

    for s in samples {
        let sem = sem.clone();
        let http = http.clone();
        let archive_url = archive_url.clone();
        let network_id = network_id.clone();
        let cache = cache.clone();
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore");
            let result = compare_tx(&db, &http, &archive_url, &network_id, &cache, &s).await;
            (s, result)
        }));
    }

    let mut report = Report::default();
    let mut diverging: Vec<TxComparison> = Vec::new();

    for h in handles {
        let (s, result) = h.await.expect("join");
        match result {
            Ok(cmp) => {
                let outcome = cmp.outcome();
                bucket_increment(
                    &mut report.by_op_count_bucket,
                    op_count_bucket(s.operation_count),
                    outcome,
                );
                bucket_increment(
                    &mut report.by_dominant_op_type,
                    cmp.dominant_op_type(),
                    outcome,
                );
                bucket_increment(
                    &mut report.by_ledger_bucket,
                    ledger_bucket(s.ledger_sequence),
                    outcome,
                );
                report.sampled += 1;
                match outcome {
                    Outcome::Match => report.matched += 1,
                    Outcome::Diverge { first_div } => {
                        report.diverged += 1;
                        *report
                            .first_div_index_histogram
                            .entry(first_div)
                            .or_insert(0) += 1;
                        diverging.push(cmp);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(hash = %s.hash_hex, ledger = s.ledger_sequence, error = %e, "compare failed");
                report.unreachable += 1;
            }
        }
    }

    if report.sampled > 0 {
        report.diverge_rate = (report.diverged as f64 / report.sampled as f64) * 100.0;
    }

    diverging.sort_by_key(|c| (op_count_bucket_index(c.operation_count), c.tx_hash.clone()));
    report.first_diverging_examples = diverging
        .into_iter()
        .take(10)
        .map(|c| {
            let outcome = c.outcome();
            let first = match outcome {
                Outcome::Diverge { first_div } => first_div,
                Outcome::Match => 0,
            };
            DivergeExample {
                tx_hash: c.tx_hash,
                operation_count: c.operation_count,
                first_div_index: first,
                db_identity: c
                    .db_order
                    .get(first)
                    .map(OpIdentity::render)
                    .unwrap_or_default(),
                xdr_identity: c
                    .xdr_order
                    .get(first)
                    .map(OpIdentity::render)
                    .unwrap_or_default(),
            }
        })
        .collect();

    Ok(report)
}

async fn compare_tx(
    db: &sqlx::PgPool,
    http: &reqwest::Client,
    archive_url: &str,
    network_id: &[u8; 32],
    cache: &Mutex<HashMap<u32, Arc<OnceCell<Arc<LedgerCloseMeta>>>>>,
    s: &SampledTx,
) -> Result<TxComparison, Box<dyn std::error::Error + Send + Sync>> {
    let meta = get_ledger_meta(http, archive_url, cache, s.ledger_sequence).await?;
    let xdr_order = extract_xdr_order(&meta, &s.hash_hex, s.ledger_sequence, network_id)?;
    let db_order = fetch_db_order(db, s.tx_id, s.created_at).await?;
    Ok(TxComparison {
        tx_hash: s.hash_hex.clone(),
        operation_count: s.operation_count,
        db_order,
        xdr_order,
    })
}

async fn get_ledger_meta(
    http: &reqwest::Client,
    archive_url: &str,
    cache: &Mutex<HashMap<u32, Arc<OnceCell<Arc<LedgerCloseMeta>>>>>,
    seq: u32,
) -> Result<Arc<LedgerCloseMeta>, Box<dyn std::error::Error + Send + Sync>> {
    // Get-or-insert the per-ledger `OnceCell`. The cache `Mutex` is held
    // only long enough to look up / insert the cell handle — the
    // (potentially slow) fetch + parse runs against a clone of the cell
    // outside the lock so that concurrent requests for *different*
    // ledgers do not serialise on the cache.
    let cell = {
        let mut guard = cache.lock().await;
        guard
            .entry(seq)
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    };
    let meta = cell
        .get_or_try_init(|| async {
            let url = ledger_url(archive_url, seq);
            let bytes = http
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            let xdr = zstd::decode_all(&bytes[..])?;
            let batch = LedgerCloseMetaBatch::from_xdr(&xdr, Limits::none())?;
            let meta = batch
                .ledger_close_metas
                .iter()
                .find(|m| ledger_seq_of(m) == seq)
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(
                        "requested ledger seq not in batch",
                    )
                })?
                .clone();
            Ok::<Arc<LedgerCloseMeta>, Box<dyn std::error::Error + Send + Sync>>(Arc::new(meta))
        })
        .await?;
    Ok(meta.clone())
}

fn extract_xdr_order(
    meta: &LedgerCloseMeta,
    target_hash_hex: &str,
    ledger_seq: u32,
    network_id: &[u8; 32],
) -> Result<Vec<OpIdentity>, Box<dyn std::error::Error + Send + Sync>> {
    let target: [u8; 32] = hex::decode(target_hash_hex)?
        .as_slice()
        .try_into()
        .map_err(|_| "tx hash not 32 bytes")?;

    let envelopes = xdr_parser::envelope::extract_envelopes(meta, network_id);
    let metas = collect_tx_metas(meta);
    let processing_hashes = tx_processing_hashes(meta);

    let idx = processing_hashes
        .iter()
        .position(|h| *h == target)
        .ok_or("tx hash not in tx_processing")?;

    let envelope = envelopes
        .get(idx)
        .and_then(Option::as_ref)
        .ok_or("envelope unaligned for tx idx")?;
    let tx_meta = metas.get(idx).copied();
    let inner = xdr_parser::envelope::inner_transaction(envelope);
    // `op_results = None` on purpose: this bin audits PG op ORDER (task 0192),
    // and `OpTypedAudit` keys identity on `liquidityPoolId` only — it ignores
    // the result-derived `poolIds`/`claimedAtoms` (task 0261), matching the PG
    // scalar `pool_id` it compares against (NULL for path payments/offers).
    // Passing the op results would change nothing here, so we skip the work.
    let ops =
        xdr_parser::extract_operations(&inner, tx_meta, None, target_hash_hex, ledger_seq, idx);

    let mut seen: HashMap<OpIdentity, ()> = HashMap::with_capacity(ops.len());
    let mut order = Vec::with_capacity(ops.len());
    for op in ops {
        let id = identity_from_extracted(&op);
        if seen.insert(id.clone(), ()).is_none() {
            order.push(id);
        }
    }
    Ok(order)
}

fn identity_from_extracted(op: &ExtractedOperation) -> OpIdentity {
    let typed = OpTypedAudit::from_details(op.op_type, &op.details);
    OpIdentity {
        op_type: op.op_type as i16,
        source: op.source_account.clone(),
        destination: typed.destination,
        contract_id: typed.contract_id,
        asset_code: typed.asset_code,
        asset_issuer: typed.asset_issuer,
        pool_id_hex: typed.pool_id_hex,
    }
}

/// Mirrors `OpTyped::from_details` from `crates/indexer/src/handler/persist/staging.rs`.
/// Kept in lockstep so the audit projection matches the DB UNIQUE constraint
/// (`uq_ops_app_identity`) byte-for-byte. If staging.rs evolves to extract
/// extra identity fields, update this struct too.
#[derive(Default)]
struct OpTypedAudit {
    destination: Option<String>,
    contract_id: Option<String>,
    asset_code: Option<String>,
    asset_issuer: Option<String>,
    pool_id_hex: Option<String>,
}

impl OpTypedAudit {
    fn from_details(op_type: OperationType, details: &Value) -> Self {
        let mut out = Self::default();
        match op_type {
            OperationType::CreateAccount | OperationType::AccountMerge => {
                out.destination = str_field(details, "destination");
            }
            OperationType::Payment => {
                out.destination = str_field(details, "destination");
                let (code, issuer) = split_asset_ref(details.get("asset"));
                out.asset_code = code;
                out.asset_issuer = issuer;
            }
            OperationType::PathPaymentStrictReceive | OperationType::PathPaymentStrictSend => {
                out.destination = str_field(details, "destination");
                let (code, issuer) = split_asset_ref(details.get("destAsset"));
                out.asset_code = code;
                out.asset_issuer = issuer;
            }
            OperationType::Clawback => {
                out.destination = str_field(details, "from");
                let (code, issuer) = split_asset_ref(details.get("asset"));
                out.asset_code = code;
                out.asset_issuer = issuer;
            }
            OperationType::LiquidityPoolDeposit | OperationType::LiquidityPoolWithdraw => {
                out.pool_id_hex = str_field(details, "liquidityPoolId");
            }
            OperationType::InvokeHostFunction => {
                out.contract_id = str_field(details, "contractId");
            }
            OperationType::ChangeTrust => {
                let (code, issuer) = split_asset_ref(details.get("asset"));
                out.asset_code = code;
                out.asset_issuer = issuer;
            }
            OperationType::SetTrustLineFlags => {
                out.destination = str_field(details, "trustor");
                let (code, issuer) = split_asset_ref(details.get("asset"));
                out.asset_code = code;
                out.asset_issuer = issuer;
            }
            OperationType::AllowTrust => {
                out.destination = str_field(details, "trustor");
                if let Some(asset) = details.get("asset")
                    && let Some(code) = asset.as_str()
                {
                    out.asset_code = Some(code.to_string());
                }
            }
            OperationType::BeginSponsoringFutureReserves => {
                out.destination = str_field(details, "sponsoredId");
            }
            _ => {}
        }
        out
    }
}

fn str_field(obj: &Value, field: &str) -> Option<String> {
    obj.get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn split_asset_ref(asset: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(s) = asset.and_then(Value::as_str) else {
        return (None, None);
    };
    if s == "native" {
        return (None, None);
    }
    match s.split_once(':') {
        Some((code, issuer)) if !code.is_empty() && !issuer.is_empty() => {
            (Some(code.to_string()), Some(issuer.to_string()))
        }
        _ => (None, None),
    }
}

async fn fetch_db_order(
    db: &sqlx::PgPool,
    transaction_id: i64,
    created_at: DateTime<Utc>,
) -> Result<Vec<OpIdentity>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT oa.type             AS op_type,
                src.account_id      AS source,
                dst.account_id      AS destination,
                sc.contract_id      AS contract_id,
                oa.asset_code       AS asset_code,
                iss.account_id      AS asset_issuer,
                encode(oa.pool_id, 'hex') AS pool_id_hex
           FROM operations_appearances oa
           LEFT JOIN accounts          src ON src.id = oa.source_id
           LEFT JOIN accounts          dst ON dst.id = oa.destination_id
           LEFT JOIN soroban_contracts sc  ON sc.id  = oa.contract_id
           LEFT JOIN accounts          iss ON iss.id = oa.asset_issuer_id
          WHERE oa.transaction_id = $1 AND oa.created_at = $2
          ORDER BY oa.id",
    )
    .bind(transaction_id)
    .bind(created_at)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OpIdentity {
            op_type: r.get("op_type"),
            source: r.get("source"),
            destination: r.get("destination"),
            contract_id: r.get("contract_id"),
            asset_code: r.get("asset_code"),
            asset_issuer: r.get("asset_issuer"),
            pool_id_hex: r.get("pool_id_hex"),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Ledger-meta helpers (independent of `archive-diff` to keep the bin self-contained)
// ---------------------------------------------------------------------------

fn ledger_url(base: &str, seq: u32) -> String {
    let p_start = seq - (seq % PARTITION_SIZE);
    let p_end = p_start + PARTITION_SIZE - 1;
    let part = format!("{:08X}--{}-{}", u32::MAX - p_start, p_start, p_end);
    let file = format!("{:08X}--{}.xdr.zst", u32::MAX - seq, seq);
    format!("{base}/{part}/{file}")
}

fn ledger_seq_of(meta: &LedgerCloseMeta) -> u32 {
    match meta {
        LedgerCloseMeta::V0(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V1(v) => v.ledger_header.header.ledger_seq,
        LedgerCloseMeta::V2(v) => v.ledger_header.header.ledger_seq,
    }
}

fn collect_tx_metas(meta: &LedgerCloseMeta) -> Vec<&TransactionMeta> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| &p.tx_apply_processing)
            .collect(),
    }
}

fn tx_processing_hashes(meta: &LedgerCloseMeta) -> Vec<[u8; 32]> {
    match meta {
        LedgerCloseMeta::V0(v) => v
            .tx_processing
            .iter()
            .map(|p| p.result.transaction_hash.0)
            .collect(),
        LedgerCloseMeta::V1(v) => v
            .tx_processing
            .iter()
            .map(|p| p.result.transaction_hash.0)
            .collect(),
        LedgerCloseMeta::V2(v) => v
            .tx_processing
            .iter()
            .map(|p| p.result.transaction_hash.0)
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
struct Report {
    sampled: usize,
    matched: usize,
    diverged: usize,
    unreachable: usize,
    diverge_rate: f64,
    by_op_count_bucket: BTreeMap<String, BucketCount>,
    by_dominant_op_type: BTreeMap<String, BucketCount>,
    by_ledger_bucket: BTreeMap<String, BucketCount>,
    first_div_index_histogram: BTreeMap<usize, usize>,
    first_diverging_examples: Vec<DivergeExample>,
}

#[derive(Debug, Default, Serialize)]
struct BucketCount {
    sampled: usize,
    diverged: usize,
}

#[derive(Debug, Serialize)]
struct DivergeExample {
    tx_hash: String,
    operation_count: i16,
    first_div_index: usize,
    db_identity: String,
    xdr_identity: String,
}

fn bucket_increment(map: &mut BTreeMap<String, BucketCount>, key: String, outcome: Outcome) {
    let entry = map.entry(key).or_default();
    entry.sampled += 1;
    if matches!(outcome, Outcome::Diverge { .. }) {
        entry.diverged += 1;
    }
}

fn op_count_bucket(n: i16) -> String {
    match n {
        2 => "2".into(),
        3..=5 => "3-5".into(),
        6..=10 => "6-10".into(),
        11..=25 => "11-25".into(),
        _ => "26+".into(),
    }
}

fn op_count_bucket_index(n: i16) -> u8 {
    match n {
        2 => 0,
        3..=5 => 1,
        6..=10 => 2,
        11..=25 => 3,
        _ => 4,
    }
}

fn ledger_bucket(seq: u32) -> String {
    let bucket = (seq / 5_000_000) * 5_000_000;
    format!("{}-{}", bucket, bucket + 4_999_999)
}

fn print_report(r: &Report) {
    println!("# audit-harness — operations-order-diff (task 0192)\n");
    println!("**Timestamp:** {}", Utc::now().to_rfc3339());
    println!("**Sampled:** {}", r.sampled);
    println!("**Matched:** {}", r.matched);
    println!("**Diverged:** {}", r.diverged);
    println!("**Unreachable:** {}", r.unreachable);
    println!("**Divergence rate:** {:.2}%\n", r.diverge_rate);

    if r.diverged == 0 && r.unreachable == 0 {
        println!("All sampled transactions match XDR apply order.");
        return;
    }

    print_bucket("Divergence by operation count", &r.by_op_count_bucket);
    print_bucket("Divergence by dominant op type", &r.by_dominant_op_type);
    print_bucket("Divergence by ledger bucket", &r.by_ledger_bucket);

    if !r.first_div_index_histogram.is_empty() {
        println!("\n## First-divergence-index histogram\n");
        println!("| index | count |");
        println!("| --- | --- |");
        for (idx, count) in &r.first_div_index_histogram {
            println!("| {idx} | {count} |");
        }
    }

    if !r.first_diverging_examples.is_empty() {
        println!("\n## Sample diverging transactions (first 10)\n");
        println!("| tx_hash | ops | first_div | db_identity | xdr_identity |");
        println!("| --- | --- | --- | --- | --- |");
        for ex in &r.first_diverging_examples {
            println!(
                "| `{}` | {} | {} | `{}` | `{}` |",
                short(&ex.tx_hash),
                ex.operation_count,
                ex.first_div_index,
                ex.db_identity,
                ex.xdr_identity
            );
        }
    }
}

fn print_bucket(title: &str, m: &BTreeMap<String, BucketCount>) {
    if m.is_empty() {
        return;
    }
    println!("\n## {title}\n");
    println!("| bucket | sampled | diverged | rate |");
    println!("| --- | --- | --- | --- |");
    for (k, v) in m {
        let rate = if v.sampled > 0 {
            (v.diverged as f64 / v.sampled as f64) * 100.0
        } else {
            0.0
        };
        println!("| `{k}` | {} | {} | {:.1}% |", v.sampled, v.diverged, rate);
    }
}
