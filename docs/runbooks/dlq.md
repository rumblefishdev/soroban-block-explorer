# DLQ — the steady state is empty

Two dead-letter queues, two level alarms (`*-ledger-processor-dlq-depth`,
`*-enrichment-dlq-depth`), one rule: **a DLQ is empty in steady state, and
any content is an event a human resolves.** Standing content is never
accepted — the historical failure mode was two alarms latched for 15 and 32
days while their content was quietly tolerated (one "cleared" only because
the messages aged out of retention).

What can legitimately land in each queue:

| Queue                  | Contents mean                                                                                                                                         | Message value                                                                  |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `ledger-processor-dlq` | reconcile failed `maxReceiveCount` times — a ClickHouse/S3 incident                                                                                   | none: doorbells carry no data (the indexer reconciles from its durable cursor) |
| `enrichment-dlq`       | a DB-write failure during a CH incident, a poison-pill message that crashes/times out the worker, or a fetch that timed out past connect on every try | redrive material / reproduction evidence                                       |

Dead issuer domains do NOT land here (since 2026-08-11): connect-level fetch
failures classify **permanent** and write the `''` sentinel immediately —
measured 30 days of "transient" retries were 100% dead domains and 0%
genuine blips, so the retry loop bought nothing. A rare host that comes back
is repaired by a `backfill-enrichment-runner --retry-sentinels` pass (run one
opportunistically alongside backfills, or quarterly).

## When the alarm fires

1. **Inspect** — SQS console → queue → "Send and receive messages" → "Poll
   for messages". Read the body and `ApproximateReceiveCount` of EVERY
   message (a later arrival may be a different failure class than the one
   that paged).
2. **Attribute** — match the arrival window against the worker/indexer logs
   (Logs Insights, `filter level="ERROR"`, 30-day retention) and
   `chq "SELECT ... FROM system.query_log"` for the DB side. Every message
   ends in: an existing task, a new task, or an evidenced one-off.
3. **Fix the cause first** — redriving into a still-broken consumer just
   cycles the messages back.
4. **Drain**:
   - `ledger-processor-dlq` → **Purge** (console: queue → Purge). Safe by
     design: doorbells are wake-ups; the next doorbell resumes reconcile
     from `max(sequence)`. Verified in practice when ~7,950 of them aged
     out on 2026-07-24 with zero ledger gaps.
   - `enrichment-dlq` → **Start DLQ redrive** (console) after the fix, so
     the enrichment actually lands. (Pre-2026-08-11 stragglers from dead
     domains are also safe to redrive — the connect failure now classifies
     permanent, so the retry writes the sentinel and acks.)
5. The alarm returns to OK on the empty queue — re-armed for the next
   event. If it pages weekly, fix the producing class; never widen the
   alarm (same policy as `docs/runbooks/api-5xx.md`).

Both drain operations are production writes — operator-run, not assistant-run.
