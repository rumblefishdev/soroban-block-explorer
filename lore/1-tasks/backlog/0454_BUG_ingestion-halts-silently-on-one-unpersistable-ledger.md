---
id: '0454'
title: 'BUG: one unpersistable ledger halts ingestion indefinitely, and no alarm can detect it'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0393', '0399', '0381', '0419', '0455']
tags:
  [
    priority-high,
    effort-medium,
    layer-indexer,
    observability,
    clickhouse,
    incident,
  ]
links:
  - crates/indexer/src/handler/mod.rs
  - crates/db-clickhouse/src/persist/writer.rs
  - infra/src/lib/stacks/cloudwatch-stack.ts
history:
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      Spawned from a 19-minute total ingestion outage on 2026-07-29 (07:39–07:58
      UTC), found by accident while watching lag during an unrelated deploy. No
      data was lost and the system self-healed, but NONE of the four alarms could
      have fired — by construction, not by misconfiguration. The root cause of the
      first failure is still unidentified; every non-reproduction evidence source
      is exhausted (enumerated below). The five robustness defects are independent
      of that unknown and are the reason this is priority-high. A sixth defect
      surfaced while writing this up: the one alarm built for exactly this
      failure (`indexer-ch-write-failures`) has been dead since the doorbell
      rewrite — it filters on a log string the code no longer emits.
---

# BUG: one unpersistable ledger halts ingestion indefinitely, undetectably

## Summary

On 2026-07-29 the indexer stopped persisting ledgers for 19 minutes. It retried
the same ledger ~200 times at 5-second intervals, reported success to Lambda every
time, and recovered only when an unrelated deploy replaced the container. Ingestion
lag reached 943 seconds. Nothing alerted.

The six defects below are what make a transient failure into a silent outage.
They are worth fixing whatever the first failure turns out to have been.

## Chronology (all times UTC)

| Time        | Event                                                                               |
| ----------- | ----------------------------------------------------------------------------------- |
| 07:09:33    | `ALTER TABLE operation_asset_appearances ADD COLUMN net_settled` (task 0419 step 1) |
| 07:09–07:38 | Ingestion normal: ~11 ledgers/min, 53–54 inserts per 5 min into the ALTERed table   |
| 07:38:56    | Ledger 63699653 closes on the network                                               |
| 07:39:00    | First `reconcile failed — will redeliver doorbell`, `error: "ClickHouse error"`     |
| 07:39–07:58 | ~200 retries of ledger 63699653, ~5s apart. Zero ledgers persisted                  |
| 07:58       | Compute stack deployed (unrelated). New container persists the same ledger fine     |
| 07:58–08:02 | Catch-up at ~5× normal rate; lag back to single-digit seconds                       |

## Evidence — what the failure actually was

Four independent layers recorded the same event from different sides:

- **Lambda**: `reconcile` fails ~480 ms after `parsing ledger`, always the same
  ledger, always the sanitised label `"ClickHouse error"`.
- **Caddy access log**: 17 requests in a 100-second sample with `status: 0`
  (response never written) and **`bytes_read: 0`**. Their URIs name the target
  table — 11 `accounts`, 2 `soroban_contracts`, 2 `transactions`, 1
  `transaction_hash_index`, 1 `transaction_participants`.
- **ClickHouse**: `UNEXPECTED_END_OF_FILE` ("while reading chunk header of HTTP
  chunked data") + `NETWORK_ERROR` (broken pipe), one pair per failed invocation,
  zero before 07:35 and zero after 08:00. **No query_log entry and no rejection** —
  the requests never became queries.
- **Data**: every ledger present, zero gaps in a 2000-ledger window.

So: the client opened insert requests, sent **no row bytes at all**, and abandoned
them. The failure is client-side, in memory, before any network I/O — the CH client
buffers rows and flushes late, so an error before the first flush aborts exactly
this way.

## Ruled out, each with evidence

- **The 07:09 ALTER** — inserts into that table continued at 53–54 per 5 min for
  29 minutes after it, until the outage. The CH client names columns explicitly.
- **`DROP TABLE assets_pre0339` (07:22:48)** — 16 minutes earlier, zero code or CH
  references to that table.
- **ClickHouse health** — uptime 22 days, 1400–1600 queries served per 5 min
  during the outage, the indexer's own `SELECT max(sequence) FROM ledgers`
  succeeded 54 times per 5 min throughout.
- **Network / proxy** — the enrichment worker wrote to the same database through
  the same proxy during the outage without a single failure.
- **Co-tenant load on the same box** — the unrelated `TOO_MANY_PARTS` counter ran
  at a constant 15 per 5 min before, during and after; it does not correlate.
- **Ledger content** — 322 transactions, 512 operations, 894 events, 192 KiB of
  event payload, all mid-range versus neighbours. No non-alphanumeric asset codes.
  Its operation types (0, 10, 16, 17) occur in 10–57 % of the surrounding 500
  ledgers.
- **Row-struct drift** — `AccountRow` is byte-identical between the deployed
  commit (`a799aa8d`) and current develop; the only `rows.rs` change in that range
  is `net_settled` on `OperationAssetAppearanceRow`. `ids.rs` gained a helper and a
  test, no behaviour change.
- **X-Ray** — the traces carry only the Lambda envelope (status 200, no
  subsegments); the CH client is not instrumented.

## Still unknown

Which row or field the pre-0393 staging code could not build. The current code
persists the same ledger without trouble, so the fix — if it is a fix — is
somewhere in the 443 changed lines of `stage.rs`. No remaining log or data source
can narrow it further.

### Reproduction (step 1 of this task)

Deterministic, offline, no production and no database — the failure happens in the
process before any I/O:

1. Check out `a799aa8d` (the commit deployed 2026-07-17 12:33 UTC).
2. Fetch the object `FC3451FF--63680000-63743999/FC34053A--63699653.xdr.zst` from
   the production ledger bucket.
3. Run the parse + stage + write path against that single object and read the real
   error, unmasked.

Outcome either way is an answer: the exact fault, or "does not reproduce" — which
would mean the trigger was transient and external, and shifts all weight onto the
robustness fixes.

## The six defects

### 1. Failure is reported as success

`handler/mod.rs` catches the error, pushes a `BatchItemFailure` and returns
`Ok(SqsBatchResponse)`. Lambda's `Errors` metric therefore stayed at **0** for the
whole outage. The partial-batch-failure mechanism is correct for a batch of
independent messages; here `batchSize` is 1 and every doorbell triggers the same
whole-pipeline reconcile, so "one item failed" and "ingestion is down" are the same
event and must not look healthy.

### 2. No alarm can detect a total stall

Every production alarm relevant to this path was verified against the incident:

| Alarm                         | Why it could not fire                                                                                                                                   |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `galexie-ingestion-lag`       | Watches for **new ledgers landing in S3**. They were landing                                                                                            |
| `ledger-processor-error-rate` | See defect 1 — the error metric stayed at zero                                                                                                          |
| `ledger-processor-dlq-depth`  | The message returned to the main queue, never to the DLQ                                                                                                |
| `IngestionLagSeconds`         | Emitted **after a successful persist**. None happened, so no data points — and `treatMissingData` is not `BREACHING` here, so "no data" is not a breach |
| `indexer-ch-write-failures`   | **Dead — see defect 6**                                                                                                                                 |

Of the seven alarms in `cloudwatch-stack.ts`, exactly one (`galexie-ingestion-lag`)
sets `treatMissingData: BREACHING`, with a comment stating it is required rather
than cosmetic. The rest read silence as health.

### 3. Unbounded retry with no escalation

~200 redeliveries of one message over 19 minutes, no backoff visible in the
5-second cadence, no cap, nothing in the DLQ, no signal anywhere.

### 4. One unpersistable ledger blocks everything, forever

The pipeline is strictly sequential by design (the ordering barrier), so a ledger
that cannot be written stops all progress with no upper bound. There is no
quarantine path and no operator escape hatch short of a redeploy.

### 5. The error label destroys the diagnosis

`safe_error_message` reduces every CH failure to `"ClickHouse error"`. The redaction
intent is right — the raw `Display` of a CH `BadResponse` can echo row values into
logs — but it also removes the error **kind**, which carries no row data. That one
word turned a 30-second diagnosis into a two-hour investigation across four systems.

### 6. The one alarm built for this failure is dead code

`IndexerChWriteFailureAlarm` exists precisely to catch "the indexer could not write
to ClickHouse". It is a metric filter matching **exact log message strings**:

```
$.fields.message = "failed to process S3 record"
$.fields.message = "failed to build mTLS CH client"
```

`failed to process S3 record` **does not exist anywhere in the codebase**. It was
removed by `bee784df` (task 0241, the SQS-doorbell rewrite) and the filter was
never updated. The message this incident actually emitted — `reconcile failed —
will redeliver doorbell` — is not in the filter either. So since that rewrite the
alarm can only fire on the cold-start mTLS path, never on a write failure.

The comment above the filter anticipates exactly this: _"any future variant
rewording would silently break the alarm"_. It was written down, and it still
happened, because nothing ever compared the filter against the strings the code
emits. That comparison is the general fix — the alarm's own string coupling is
only the instance.

The signal that did track the outage perfectly was
`ApproximateAgeOfOldestMessage` on `production-ledger-ingest`: 0 s before, then
31 → 328 → 578 → 916 → 1231 → 1421 s. AWS emits it for free and nothing watches it.

## Implementation

1. **Reproduce** (above) and fix the underlying fault, or record "does not
   reproduce" with the evidence.
2. **Log the error kind**, not the payload — the `clickhouse::error::Error` variant
   name is safe to emit; keep the value redaction as-is.
3. **Alarm on `ApproximateAgeOfOldestMessage`** for `production-ledger-ingest`
   (threshold well under the current 1421 s peak — a few minutes). This is the one
   change that would have caught this incident.
4. **Make the failure visible in the error metric** — either fail the invocation
   when the reconcile itself failed (batchSize is 1, so nothing else is lost), or
   emit an explicit failure metric alongside the batch-item response.
5. **Bound the retry** — cap redeliveries, then DLQ, and alarm on it.
6. **Consider a quarantine path** for a ledger that fails N times, so one bad
   ledger degrades instead of halting. Design decision, not obviously worth it —
   record the reasoning either way.
7. **Repair `indexer-ch-write-failures`** and stop it drifting again: match on a
   stable field rather than prose (a dedicated `event` / error-kind field), and
   add a check that every string an alarm filters on still exists in the code.
   The generic version of that check belongs to the umbrella task [[0455]].

## Acceptance Criteria

- [ ] Reproduction run and its outcome recorded (fault identified, or explicitly
      "does not reproduce" with the evidence)
- [ ] Indexer logs the CH error kind; no row values in logs (verify on a forced
      failure)
- [ ] An alarm exists that fires on ingestion stall — verified by simulating a
      stall, not by reading the config
- [ ] A failing reconcile is visible in a metric an alarm can watch
- [ ] Retry is bounded and exhaustion is alarmed
- [ ] `indexer-ch-write-failures` fires on a forced write failure — verified by
      forcing one, not by reading the filter
- [ ] Quarantine decision recorded (implemented, or declined with reason)
- [ ] **Docs updated** — `docs/runbooks/**` gains an "ingestion stalled" entry:
      how to recognise it, which queries confirm it, how to clear it
- [ ] **API types regenerated** — N/A, nothing under `crates/api/**`,
      `Cargo.{toml,lock}` or `libs/api-types/**`

## Notes

- The 19-minute outage was found only because someone happened to be watching lag
  after an unrelated deploy. At night it would have run until morning.
- Recovery coincided exactly with the container being replaced. That is suggestive
  of process-local state, but `PartitionWriter` is constructed per run and the CH
  client's pool is shared with the reads that kept working — so it is a lead, not
  a conclusion.
- Investigation cost: ~2 hours across CloudWatch, X-Ray, the Caddy access log, the
  ClickHouse `system.error_log` / `text_log`, and the git history. Defect 5 is why.
