---
id: '0455'
title: 'OPS: observability umbrella — "declared vs actual, never compared" and "health measured by success"'
type: OPS
status: backlog
related_adr: []
related_tasks:
  [
    '0454',
    '0428',
    '0400',
    '0312',
    '0434',
    '0232',
    '0406',
    '0250',
    '0237',
    '0449',
    '0448',
    '0447',
    '0403',
    '0382',
    '0087',
    '0127',
  ]
tags: [priority-high, effort-large, observability, ops, umbrella, cross-project]
links:
  - infra/src/lib/stacks/cloudwatch-stack.ts
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      Spawned after two investigations in two days, both of which ended in the
      same place: the data existed, but nothing compared it or alarmed on it, so
      it took hours of manual queries across six systems to find. Sixteen open
      tasks turned out to be instances of two recurring defects rather than
      sixteen separate problems. This task owns the two defects; the children own
      their instances.
---

# Observability umbrella — two recurring defects, sixteen instances

## Summary

Sixteen open tasks describe what looks like sixteen problems. They are two:

1. **Declared vs actual, never compared.** We write down how things should be,
   reality drifts, and nothing continuously checks one against the other.
2. **Health measured by success, so failure is silent.** Signals are emitted on
   the success path, so when a thing breaks its signal does not go bad — it goes
   _absent_, and absence is not an alarm state by default.

Both were demonstrated in production this week. Neither is a monitoring-tooling
gap: the data existed in every case. What is missing is a comparator and an
inverted signal.

## Evidence — the two incidents that produced this task

**2026-07-29, ingestion stalled 19 minutes ([[0454]]).** Total ingestion outage.
Seven alarms; none could fire. The one alarm built for exactly this failure
(`indexer-ch-write-failures`) filters on the log string `failed to process S3
record`, which has not existed in the codebase since the doorbell rewrite
(`bee784df`). The comment above that filter predicted this: _"any future variant
rewording would silently break the alarm."_ Nothing ever compared the filter to
the strings the code emits. Found by accident. ~2 hours to diagnose across
CloudWatch, X-Ray, the Caddy access log, ClickHouse `system.error_log` /
`text_log`, and git history.

**2026-07-28, AWS cost spike.** Egress grew by about two orders of magnitude
month over month (figures in [[0449]]). Roughly a day to trace, because the
account bills two projects as one — the origin turned out to be outside this
repository, so no amount of looking here could have found it.

Both are the same shape: the signal existed, nobody was comparing or watching it.

## Defect 1 — declared vs actual, never compared

| Declared in             | Actual                      | Instance                                                   |
| ----------------------- | --------------------------- | ---------------------------------------------------------- |
| `init.sql`              | prod schema                 | [[0400]]                                                   |
| CDK app                 | deployed stacks             | [[0312]] — pending 7 weeks, found during an unrelated diff |
| alarm filter strings    | strings the code emits      | [[0454]] defect 6 — dead since the rewrite                 |
| protocol tables in code | the chain                   | [[0434]]                                                   |
| column contract         | live-mode writes            | [[0232]]                                                   |
| test files in repo      | what CI runs                | [[0406]] — 25 e2e files no pipeline has executed           |
| read quotas             | one auth path bypasses them | [[0250]]                                                   |

Fix shape: **one periodic comparator**, many subjects. Emit the deltas, fail
loudly. It replaces the "discover by accident, weeks later" mode with a
scheduled report, and it retires most of the rows above at once.

## Defect 2 — health measured by success

| Signal                | Why silence looks healthy                                                      | Instance |
| --------------------- | ------------------------------------------------------------------------------ | -------- |
| `IngestionLagSeconds` | emitted after a successful persist                                             | [[0454]] |
| refreshable MV        | reports rows from metadata; a failed refresh is indistinguishable from success | [[0428]] |
| host reboot flag      | the box knows; nothing asks                                                    | [[0237]] |

Of the seven alarms in `cloudwatch-stack.ts`, exactly one sets
`treatMissingData: BREACHING`. Fix shape: **watch upstream of the thing that
breaks, and treat absence as failure.** `ApproximateAgeOfOldestMessage` on the
ingest queue tracked the 0454 outage perfectly (0 → 1421 s) and is emitted by AWS
for free; nothing watches it.

## Defect 3 (thinner) — no owner dimension

Shared AWS account and shared ClickHouse box, no attribution ([[0449]]). Any
"who caused this" question becomes an investigation, and neither side has a
signal for the other: a co-tenant's load is visible to us only if someone
happens to read a counter, and ours is equally invisible to them.

## Implementation — ordered by return

### 1. Read-only credentials for the assistant workspace (hours, no code)

The single biggest cost in both investigations was that commands had to be
hand-relayed. ClickHouse was already fast because `chq` is available locally; the
AWS half was not. A read-only IAM principal removes that asymmetry:

```
logs:FilterLogEvents, logs:StartQuery, logs:GetQueryResults, logs:DescribeLogGroups
cloudwatch:GetMetricStatistics, cloudwatch:GetMetricData, cloudwatch:DescribeAlarms
sqs:GetQueueAttributes, sqs:ListQueues
lambda:GetFunctionConfiguration, lambda:ListFunctions
cloudformation:DescribeStacks
ce:GetCostAndUsage
xray:BatchGetTraces
```

No write, no IAM, no secret reads. SSH to the box stays manual and human-run.

### 2. Invert the signals (small, one file)

- alarm on `ApproximateAgeOfOldestMessage` for `production-ledger-ingest`
- `treatMissingData: BREACHING` where absence genuinely means failure
- MV freshness measured from **data recency**, not metadata ([[0428]])
- repair `indexer-ch-write-failures` and stop matching on prose

### 3. The comparator (about a day)

One scheduled job, printing deltas for every row of defect 1. Start with the two
that already bit us — schema and CDK — then alarm filters vs emitted strings, then
tests vs CI.

### 4. Cost attribution tags ([[0449]])

Turns yesterday's day-long investigation into a group-by.

## Tooling decision — no external log platform yet

Considered and rejected for now (Datadog / Grafana Cloud / Axiom):

- Neither incident would have been caught by one. Both needed an alarm on
  **absence**, not a log search.
- Shipping logs off Hetzner is the same egress line that just cost $337; the Caddy
  access log alone holds 11.5M entries.
- It adds a vendor, a bill and a secret to rotate, for two people.

If a single sink is wanted later, **CloudWatch** is the default: the Lambdas are
already there, it is one credential surface and one alarm engine, and the box can
ship a filtered stream into it. **ClickHouse is explicitly rejected as the sink for
the critical path** — observability living inside the system it observes goes
quiet exactly when it is needed. Revisit the external option when there are more
than two people on call, or when retention beyond CloudWatch's is required.

## Acceptance Criteria

- [ ] Read-only AWS principal exists and is usable from the assistant workspace
- [ ] An alarm fires on ingestion stall — verified by simulating a stall
- [ ] Every alarm's `treatMissingData` reviewed and justified in a comment
- [ ] Comparator runs on a schedule and reports schema + CDK deltas; its output is
      seen by a human without anyone asking for it
- [ ] Alarm filter strings verified against the strings the code actually emits
- [ ] Cost allocation tags applied; a per-project cost answer takes minutes
- [ ] Each child task either closed by this work or explicitly re-scoped
- [ ] **Docs updated** — `docs/runbooks/**` gains "how do I tell if it is broken",
      naming the signals and where they live
- [ ] **API types regenerated** — N/A, no API surface change

## Notes

- Deliberately NOT in scope: replacing CloudWatch, dashboards, tracing coverage,
  frontend telemetry ([[0087]]). Those are worth doing and none of them would have
  caught either incident.
- The cross-project half needs the other team: shared infrastructure with no
  shared signal means each side is blind to the other. Minimum viable version is
  one health signal for the box that both teams can see.
