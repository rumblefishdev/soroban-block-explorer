//! Self-pacing for an indexer that reads the public data lake (task 0553).
//!
//! On mainnet each ledger file our Galexie writes rings the indexer through
//! an S3 event — one doorbell per ledger. The public data lake sends no
//! events, so the indexer rings itself: after each wake it queues ONE message,
//! delayed to when the next ledger's file should have landed, naming the
//! ledger it expects.
//!
//! A once-a-minute keepalive (`infra/src/lib/stacks/public-lake-keepalive.ts`)
//! restarts the chain when it has died — a failed message, a lake outage. It
//! starts a chain only when it stored new ledgers itself, so a stalled lake
//! does not breed chains. Two chains merge on their own: a message whose
//! ledger another chain already stored is dropped without a successor.
//!
//! Mainnet never builds a [`Pacer`], and its doorbell bodies stay ignored.

use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_sqs::Client as SqsClient;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::{HandlerError, Reconciled};

const INGEST_QUEUE_URL_ENV: &str = "INGEST_QUEUE_URL";
/// Stellar closes a ledger about every 5 s.
// ponytail: the network's target, not a measurement; measure the interval if
// testnet's cadence ever drifts from it.
const LEDGER_INTERVAL_SECS: i64 = 5;
/// Seconds after the next ledger's expected close before its file is looked
/// for. Measured 2026-09-30 over an hour of testnet: a file lands in the lake
/// p50 2.4 s and p90 3.2 s after its ledger closes.
const LAKE_MARGIN_SECS: i64 = 3;
/// Longest wait between two looks, on time or retrying a late file.
const MAX_DELAY_SECS: i64 = 15;

/// Body of a chain message: the ledger this wake expects, and how many looks
/// already found its file missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expect {
    pub expect: i64,
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

/// The one message to queue, and after how many seconds SQS delivers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Next {
    pub message: Expect,
    pub delay_secs: i32,
}

/// The message this wake leaves behind, if any. `newest_closed_at` is the
/// close time of the newest ledger, read only when this wake stored ledgers.
pub fn next(wake: Wake, done: Reconciled, newest_closed_at: Option<i64>, now: i64) -> Option<Next> {
    if let Some(closed_at) = newest_closed_at {
        // This wake stored ledgers: expect the one after the newest.
        return Some(Next {
            message: Expect {
                expect: done.newest + 1,
                attempt: 0,
            },
            delay_secs: (closed_at + LEDGER_INTERVAL_SECS + LAKE_MARGIN_SECS - now)
                .clamp(1, MAX_DELAY_SECS) as i32,
        });
    }
    match wake {
        // The expected file is not in the lake yet: look again.
        Wake::Chain(e) if done.newest > 0 && done.newest < e.expect => {
            let attempt = e.attempt + 1;
            Some(Next {
                message: Expect {
                    expect: e.expect,
                    attempt,
                },
                delay_secs: backoff(attempt),
            })
        }
        // Another chain already stored it, the keepalive found nothing new,
        // or the database is empty.
        _ => None,
    }
}

/// 1, 2, 4, 8, then 15 s between looks at a file that is late.
fn backoff(attempt: u32) -> i32 {
    (1_i64 << (attempt.min(5) - 1)).min(MAX_DELAY_SECS) as i32
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
        if queue_url.is_empty() {
            return Err(format!("{INGEST_QUEUE_URL_ENV} must not be empty"));
        }
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

/// Close time of one stored ledger, in Unix seconds. A primary-key lookup;
/// duplicate rows an unmerged `ReplacingMergeTree` still holds share it.
async fn closed_at(ch: &clickhouse::Client, sequence: i64) -> Result<i64, HandlerError> {
    let closed = ch
        .query("SELECT toInt64(toUnixTimestamp(closed_at)) FROM ledgers WHERE sequence = ? LIMIT 1")
        .bind(sequence)
        .fetch_one()
        .await
        .map_err(db_clickhouse::SchemaError::Query)?;
    Ok(closed)
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
    Fut: Future<Output = Result<Reconciled, HandlerError>>,
{
    let wake = Wake::parse(body);
    let done = reconcile().await?;
    let newest_closed_at = if done.persisted > 0 {
        Some(closed_at(ch, done.newest).await?)
    } else {
        None
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    match next(wake, done, newest_closed_at, now) {
        Some(next) => {
            if let Err(e) = pacer.send(next).await {
                warn!(error = %e, ?next, "could not queue the next wake-up — the keepalive restarts the chain");
            }
        }
        None => info!(?wake, newest = done.newest, "no next wake-up queued"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
