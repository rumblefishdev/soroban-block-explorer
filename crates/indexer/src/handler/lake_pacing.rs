//! Self-pacing for an indexer that reads the public data lake (task 0553).
//!
//! On mainnet each ledger file our Galexie writes rings the indexer through
//! an S3 event — one doorbell per ledger. The public data lake sends no
//! events, and EventBridge Scheduler fires anywhere inside its minute, so no
//! schedule can ring once per ledger. The indexer rings itself instead: after
//! each wake it queues ONE message, delayed to when the next ledger's file
//! should have landed, naming the ledger it expects.
//!
//! A once-a-minute keepalive (`infra/src/lib/stacks/public-lake-keepalive.ts`)
//! restarts the chain when it has died — a failed message, a lake outage. It
//! starts a chain only when it found new ledgers itself, so a stalled lake
//! does not breed chains. Two chains merge on their own: a message whose
//! ledger is already stored is dropped without a successor.
//!
//! Mainnet never builds a [`Pacer`], and its doorbell bodies stay ignored.

use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_sqs::Client as SqsClient;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::HandlerError;

const INGEST_QUEUE_URL_ENV: &str = "INGEST_QUEUE_URL";
/// Seconds after the next ledger's expected close before its file is looked
/// for. Measured 2026-09-30 over an hour of testnet: a file lands in the lake
/// p50 2.4 s and p90 3.2 s after its ledger closes.
const LAKE_MARGIN_SECS: i64 = 3;
/// Used until two ledgers are stored; Stellar closes one every ~5 s.
const DEFAULT_INTERVAL_SECS: i64 = 5;
/// Longest wait between two looks, on time or retrying a late file.
const MAX_DELAY_SECS: i64 = 15;

/// Body of a chain message: the ledger this wake expects, and how many looks
/// already found its file missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expect {
    pub expect: i64,
    #[serde(default)]
    pub attempt: u32,
}

/// Why the indexer woke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Chain(Expect),
    /// The scheduled keepalive — and any body that is not a chain message.
    Keepalive,
}

impl Wake {
    pub fn parse(body: Option<&str>) -> Self {
        body.and_then(|b| serde_json::from_str::<Expect>(b).ok())
            .map_or(Wake::Keepalive, Wake::Chain)
    }
}

/// The newest stored ledger, and the seconds between it and the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tip {
    pub sequence: i64,
    pub closed_at: i64,
    pub interval: i64,
}

/// The one message to queue, and after how many seconds SQS delivers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Next {
    pub message: Expect,
    pub delay_secs: i32,
}

/// A chain message whose ledger is already stored: another chain served it.
pub fn already_served(wake: Wake, stored: i64) -> bool {
    matches!(wake, Wake::Chain(e) if e.expect <= stored)
}

/// The message this wake leaves behind, if any.
pub fn next(wake: Wake, stored_before: i64, tip: Option<Tip>, now: i64) -> Option<Next> {
    let tip = tip?;
    let on_time = Next {
        message: Expect {
            expect: tip.sequence + 1,
            attempt: 0,
        },
        delay_secs: delay_until_next(tip, now),
    };
    match wake {
        Wake::Chain(e) if tip.sequence >= e.expect => Some(on_time),
        Wake::Chain(e) => {
            let attempt = e.attempt + 1;
            Some(Next {
                message: Expect {
                    expect: e.expect,
                    attempt,
                },
                delay_secs: backoff(attempt),
            })
        }
        Wake::Keepalive if tip.sequence > stored_before => Some(on_time),
        Wake::Keepalive => None,
    }
}

fn delay_until_next(tip: Tip, now: i64) -> i32 {
    let interval = if tip.interval > 0 {
        tip.interval
    } else {
        DEFAULT_INTERVAL_SECS
    };
    (tip.closed_at + interval + LAKE_MARGIN_SECS - now).clamp(1, MAX_DELAY_SECS) as i32
}

/// 1, 2, 4, 8, then 15 s between looks at a file that is late.
fn backoff(attempt: u32) -> i32 {
    (1_i64 << (attempt.clamp(1, 5) - 1)).min(MAX_DELAY_SECS) as i32
}

/// Queues the indexer's next wake-up on its own ingest queue.
#[derive(Clone)]
pub struct Pacer {
    client: SqsClient,
    queue_url: String,
}

impl Pacer {
    pub fn from_env(client: SqsClient) -> Result<Self, String> {
        let queue_url = std::env::var(INGEST_QUEUE_URL_ENV)
            .map_err(|_| format!("{INGEST_QUEUE_URL_ENV} must be set to read the data lake"))?;
        Ok(Self { client, queue_url })
    }

    async fn send(&self, next: Next) -> Result<(), String> {
        let body = serde_json::to_string(&next.message).map_err(|e| e.to_string())?;
        self.client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .delay_seconds(next.delay_secs)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// The newest stored ledger. `LIMIT 1 BY` collapses the duplicate rows an
/// unmerged `ReplacingMergeTree` still holds.
pub async fn tip(ch: &clickhouse::Client) -> Result<Option<Tip>, clickhouse::error::Error> {
    #[derive(clickhouse::Row, Deserialize)]
    struct Row {
        sequence: i64,
        closed: i64,
    }
    let rows: Vec<Row> = ch
        .query(
            "SELECT sequence, toInt64(toUnixTimestamp(closed_at)) AS closed \
             FROM ledgers ORDER BY sequence DESC LIMIT 1 BY sequence LIMIT 2",
        )
        .fetch_all()
        .await?;
    Ok(rows.first().map(|newest| Tip {
        sequence: newest.sequence,
        closed_at: newest.closed,
        interval: rows
            .get(1)
            .map_or(0, |before| newest.closed - before.closed),
    }))
}

/// Runs `reconcile` for one wake and queues the next one.
///
/// A reconcile failure returns the error and queues nothing: SQS redelivers
/// the message later, and the keepalive restarts the chain meanwhile.
pub async fn paced<F, Fut>(
    pacer: &Pacer,
    ch: &clickhouse::Client,
    body: Option<&str>,
    reconcile: F,
) -> Result<(), HandlerError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), HandlerError>>,
{
    let as_ch_error = |e| HandlerError::ClickHouse(db_clickhouse::SchemaError::Query(e));
    let wake = Wake::parse(body);
    let stored = tip(ch)
        .await
        .map_err(as_ch_error)?
        .map_or(0, |t| t.sequence);
    if already_served(wake, stored) {
        info!(
            ?wake,
            stored, "ledger already served by another chain — dropped"
        );
        return Ok(());
    }

    reconcile().await?;

    let after = tip(ch).await.map_err(as_ch_error)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if let Some(next) = next(wake, stored, after, now)
        && let Err(e) = pacer.send(next).await
    {
        warn!(error = %e, ?next, "could not queue the next wake-up — the keepalive restarts the chain");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
