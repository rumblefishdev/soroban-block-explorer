# Health — how do I tell if it is broken?

The single entry point for "something looks wrong". It names every signal
and where it lives, so an investigation starts from a map instead of from
memory. Specialised procedures branch off it: [api-5xx.md](./api-5xx.md),
[dlq.md](./dlq.md), [costs.md](./costs.md), the pause procedure in
[deployment.md](../deployment.md), and the post-cutover watchlist in
[live-tail-cutover.md](./live-tail-cutover.md).

## The four sentences (what each surface is FOR)

These are the rules every signal below follows; a new signal that cannot
say which sentence it serves is a smell (ADR 0054 carries the alarm-side
rules).

1. **Logs answer WHY**, once something is known to be wrong. Variables
   belong in structured fields; detection never reads log text (the one
   metric filter keys on a structured field, CI-guarded).
2. **Metrics answer WHETHER.** Alarms read metrics only.
3. **The dashboard answers WHERE.** It carries what the alarms read, plus
   standing conditions deliberately left unalarmed.
4. **What pages is on the dashboard; what is on the dashboard comes from a
   metric; a metric exists because a component published it deliberately.**

## Coverage matrix (every cell DECIDED, not necessarily filled)

`✅` = exists and effective · `➖` = deliberately none, reason given ·
`🔜` = in code on this branch, live after the next deploy.

| Condition                                           | Alarm                                                                                                                                                                                       | Metric                               | Logs (WHY)                                            | Dashboard                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ----------------------------------------------------- | -------------------------------------------------- |
| Galexie stops writing ledgers                       | ✅ `galexie-ingestion-lag` (BREACHING on silence)                                                                                                                                           | SQS `NumberOfMessagesSent`           | ECS logs, 90 d                                        | 🔜 doorbell-rate widget (same signal as the alarm) |
| Galexie disk fills                                  | ✅ `galexie-ephemeral-storage` (level; re-arm = act before ceiling)                                                                                                                         | Container Insights %                 | ECS logs                                              | ➖ (C7 candidate)                                  |
| Ledgers queued but not consumed (the 0454 gap)      | 🔜 `ingestion-backlog-age` (bare 120 s × 3 min; a planned pause pages once, knowingly)                                                                                                      | SQS `ApproximateAgeOfOldestMessage`  | indexer ERROR logs, full error detail                 | 🔜 backlog-age widget (threshold line drawn)       |
| Indexer fails a CH write                            | ✅ `indexer-ch-write-failures` (filter on `fields.alarm`, CI-guarded; 🔜 zero-tolerance — any single post-retry failure) + error-rate alarm                                                 | 2 custom metrics                     | `alarm="ch_write_failure"` events with full error     | 🔜 errors + CH-write-failures widget               |
| Ingest DLQ receives                                 | ✅ `ledger-processor-dlq-depth` (level; steady state zero; drain = purge per [dlq.md](./dlq.md))                                                                                            | SQS depth                            | indexer logs                                          | depth widget                                       |
| Enrichment fetch dead-ends (dead issuer domain)     | ➖ deliberate: classifies permanent → sentinel row, not an incident                                                                                                                         | —                                    | worker WARN `reason=sep1_fetch_permanent`/`transient` | ➖                                                 |
| Enrichment DLQ receives (DB incident / poison pill) | ✅ `enrichment-dlq-depth` (level; drain = redrive per [dlq.md](./dlq.md))                                                                                                                   | SQS depth                            | worker ERROR logs                                     | depth widget                                       |
| API request returns 5xx                             | ✅ `api-gateway-5xx` (any single one; zero-tolerance per [api-5xx.md](./api-5xx.md))                                                                                                        | APIGW `5XXError`                     | API ERROR logs, structured fields                     | 4xx/5xx widget                                     |
| API slow                                            | ➖ deliberate: latency is not a page for this team; hard stall becomes 504 → 5xx alarm                                                                                                      | APIGW `Latency`                      | REPORT lines                                          | latency widget                                     |
| `accounts_recent` MV refresh silently fails         | ➖ deliberate (measured 694/693 clean hours; alarm design + return conditions in task 0428)                                                                                                 | —                                    | —                                                     | ➖ — diagnosis query below                         |
| Costs step-change                                   | 🔜 Cost Anomaly Detection → SNS ([costs.md](./costs.md))                                                                                                                                    | billing data                         | —                                                     | ➖ (C7: stated answer pending)                     |
| Database host (Hetzner) degrades                    | ➖ known gap, task 0237 — indicated shape is an external dead-man's-switch (ADR 0054), NOT CloudWatch. Cost measured 2026-08-14: disk pressure stopped ingestion ~9.5 h with one quiet page | Prometheus on the box, not forwarded | on the box                                            | ➖                                                 |
| Frontend errors                                     | ➖ out of the umbrella's scope (task 0087 owns it when activated)                                                                                                                           | —                                    | —                                                     | ➖                                                 |
| Slack delivery chain itself breaks                  | ➖ OPEN DECISION (ADR 0054: single unwitnessed chain; email co-subscriber / dead-man's-switch / nothing)                                                                                    | —                                    | —                                                     | ➖                                                 |

## Symptom → first move

- **"Ingestion is behind"** → alarm history of `galexie-ingestion-lag`
  (producer) vs `ingestion-backlog-age` (consumer): lag = Galexie side,
  backlog-age = indexer side. Then indexer Logs Insights:
  `filter level="ERROR"` — since 2026-08-10 errors carry FULL ClickHouse
  detail (the old sanitizer is gone). Cross-check the DB side:
  `chq "SELECT event_time, exception_code, left(exception,140) FROM
system.query_log WHERE event_date=today() AND exception_code!=0 ORDER BY
event_time DESC LIMIT 20"`. NOTE: a backlog-age page right after you
  paused the indexer is expected and correct — one knowing page per pause
  (ADR 0054 rule 4); it doubles as the forgot-to-re-enable bound.
- **"A `galexie-ingestion-lag` page arrived"** → first check for a task
  restart: `/ecs/production/galexie-live` logs, look for
  `Starting Galexie` around the page time, and the ECS service events
  (`aws ecs describe-services --cluster production-ingestion --services
production-galexie-live --query 'services[0].events[0:5]'`). A
  stop+start pair with NO deployment = AWS-initiated Fargate task
  replacement (host patching) — measured 2026-08-12: ~25 min from restart
  to full catch-up, one page, one recovery message, zero ledger gaps.
  That is the expected envelope, not an incident; afterwards verify
  continuity: `chq "SELECT count() FROM (SELECT sequence,
lagInFrame(sequence) OVER (ORDER BY sequence ROWS BETWEEN 1 PRECEDING
AND CURRENT ROW) AS prev FROM ledgers WHERE closed_at >
now() - INTERVAL 3 HOUR) WHERE prev != 0 AND sequence - prev > 1"`
  (expect 0; the ROWS frame AND the `prev != 0` filter are both
  load-bearing — without them the window's first row fakes a gap, which
  bit twice before this phrasing).
- **"Indexer errors say `Code: 243` / NOT_ENOUGH_SPACE"** → the shared
  ClickHouse box is out of disk (materialized 2026-08-14: ~9.5 h of
  stopped ingestion whose only page was the DLQ level alarm, hours in).
  First moves: `chq "SELECT name, formatReadableSize(free_space),
round(100*(1-free_space/total_space),1) AS used_pct FROM system.disks"`
  and `chq "SELECT user, left(query,60), count() FROM system.query_log
WHERE exception_code = 243 AND event_time > now() - INTERVAL 12 HOUR
GROUP BY 1,2 ORDER BY 3 DESC"` — code 243 hits EVERY writer on the box
  (both tenants), so also check for an operator bulk job (a backfill
  staging data on the box's own disk is the known cause class). Freeing
  space is a box-side operator action; the indexer then self-heals from
  its cursor with no intervention (measured catch-up ≈ 7-8× realtime) —
  afterwards purge the accumulated DLQ doorbells per [dlq.md](./dlq.md)
  and run the continuity query above.
- **"A 5xx page arrived"** → [api-5xx.md](./api-5xx.md), every error gets
  an owner.
- **"A DLQ page arrived"** → [dlq.md](./dlq.md), inspect → attribute →
  fix → drain.
- **"Accounts list / total-accounts KPI look stale"** →
  `chq "SELECT view, status, last_success_time, exception, retry FROM
system.view_refreshes WHERE view LIKE '%accounts_recent%'"` — healthy is
  `Scheduled`, empty exception, `last_success_time` within ~2 min. Stale +
  exception → task 0428 holds the ready alarm design (that observation is
  its return condition).
- **"The bill looks wrong"** → [costs.md](./costs.md).
- **"Slack has been quiet a long time"** → quiet usually means healthy,
  but the chain has no witness (open decision above). Manual check:
  CloudWatch console → Alarms (any ALARM without a Slack message = chain
  broken) and Chatbot "Send test message" after every CloudWatch deploy.

## When nothing above matches (the escape hatch)

The list above is finite; failures are not. General toolkit, in the order
that has actually worked:

1. **Alarm state history** — `aws cloudwatch describe-alarm-history` (90 d)
   — what changed state around the symptom window, including transitions
   nobody was paged for (OK→OK re-evaluations excluded by design).
2. **Logs Insights per component** — groups
   `/aws/lambda/production-soroban-explorer-{api,indexer,enrichment-worker}`
   (30-day retention — anything older is unanswerable, see below):
   `filter level in ["ERROR","WARN"] | stats count(*) by fields.message,
fields.error | sort count desc`. Cold starts live here too —
   `InitDuration` is NOT a CloudWatch metric (the widget removed in 0455
   graphed one that never existed), but Logs Insights parses it from every
   REPORT line: `filter @type = "REPORT" | stats count(@initDuration),
avg(@initDuration), max(@initDuration)`.
3. **ClickHouse system tables via `chq`** — `system.query_log` (who failed
   what, ~85 d), `system.part_log` (write cadence per table, ~85 d),
   `system.view_refreshes` (refresh health, current), `system.errors`
   (counters since restart). Remember RMT dedup rules when reading data
   tables.
4. **Deploy correlation** — `git log` on `infra/` + CloudFormation stack
   events; a symptom that starts at a deploy boundary usually is one.
5. **The co-tenant** — the box is shared; their write pressure has shown up
   in our windows before. `system.query_log` filtered on their database
   answers it read-only.

Hard bounds every investigation inherits: **Lambda logs 30 days**, **no
CloudTrail trail** (only the free 90-day event history), alarm history 90
days. Anything older is unanswerable by construction.

## The feedback rule

Every incident and every page ends with one question: **what did this
runbook not know?** The answer gets added — a new matrix row, a new symptom
path, a new escape-hatch tool. A runbook that stops growing turns into a
tunnel; this one has the obligation to grow written in (same zero-tolerance
loop as api-5xx and dlq: every event gets an owner, every gap gets a line).
