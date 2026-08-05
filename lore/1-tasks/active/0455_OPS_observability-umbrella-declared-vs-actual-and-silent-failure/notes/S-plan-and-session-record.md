---
prefix: S
title: Coverage matrix, inconsistencies, and the ordered step list
status: mature
---

# S — Where this stands and what the steps are

Companion to [R — measured state](R-deep-review-findings-2026-08-04.md), which
holds the measurements. This note holds the conclusions and the work list.

## Scope decision

Two decisions frame everything below.

**The scope is four surfaces, not the alarm set.** Logs, metrics, dashboard and
alarms each have gaps, and the gaps between them are as large as the gaps within
any one of them.

**Existing alarms and widgets are kept.** Where a signal cannot currently fire,
the work is to make the condition reach it, not to remove the signal. Removing a
signal that cannot fire removes the evidence and leaves the cause in place. This
also produces a smaller diff: an alarm reading Lambda `Errors` starts working
once a failed reconcile reaches that metric, with no change to the alarm.

---

## Coverage matrix

Rows are conditions that can occur; columns are the four surfaces. A uniform
system would show a repeating pattern across rows.

| Condition                 | Logs                                 | Metric                    | Dashboard           | Alarm                                                                                      |
| ------------------------- | ------------------------------------ | ------------------------- | ------------------- | ------------------------------------------------------------------------------------------ |
| Galexie output stops      | ECS, 90 days                         | AWS-native                | no ephemeral widget | lag + disk, both effective                                                                 |
| Ingest queue backs up     | —                                    | AWS-native, age collected | none                | producer side only; age unread                                                             |
| Indexer fails a write     | level set, structured fields         | 2 custom                  | partial             | 2 alarms; one matches a removed string, one reads a metric the failure path does not reach |
| Ingest DLQ receives       | —                                    | AWS-native                | depth               | latched 15 days                                                                            |
| Enrichment queue backs up | —                                    | AWS-native                | none                | none                                                                                       |
| Enrichment fetch fails    | level set, structured fields         | none of its own           | none                | reads a metric the failure path does not reach                                             |
| Enrichment DLQ receives   | —                                    | AWS-native                | depth               | latched 32 days                                                                            |
| API query fails           | ERROR only; variables inside message | none of its own           | latency             | none on the function; gateway 5xx carries the volume                                       |
| Database host degrades    | on the host only                     | Prometheus, not forwarded | none                | none                                                                                       |
| Frontend errors           | none                                 | none                      | none                | none                                                                                       |

Three readings:

1. **Rows differ from each other.** The three Lambdas are the same kind of
   component with three different coverage levels.
2. **The metric column is nearly complete; the alarm column is not.** The data
   is largely collected already — ingest queue age is an AWS-native metric that
   has always existed. What is missing is a reader.
3. **Dashboard and alarm do not correspond in any row.** Where a widget exists
   there is often no alarm, and two alarms have no widget.

---

## The largest inconsistencies

Ordered by how much follows from fixing each.

### I1. The same kind of component is instrumented three ways

|                   | `RUST_LOG` | Own metrics | Log filter | Alarm on own metrics |
| ----------------- | ---------- | ----------- | ---------- | -------------------- |
| Indexer           | set        | yes         | yes        | 2                    |
| Enrichment worker | set        | no          | no         | 2                    |
| API               | not set    | no          | no         | none                 |

Each was instrumented alongside the work that introduced it. Nothing states what
a component should publish, so each has what its own task needed.

### I2. Detection reads log text in one place and metrics everywhere else

One `logs.MetricFilter` exists in the CDK app. It matches an exact message
string, which pairs a value in TypeScript with a value in Rust across two
deployment units with no comparison between them. Every other detection path
reads a metric.

### I3. Dashboard and alarms were assembled independently

Seven widgets have no alarm; two alarms have no widget. `Galexie S3 freshness`
graphs Lambda invocations while the alarm for the same question reads SQS
`NumberOfMessagesSent` — task 0367 moved the alarm off the invocation signal and
the widget stayed. `API Gateway cache hit / miss` reads a cache that
`apiGatewayCacheEnabled: false` leaves unprovisioned.

### I4. Structured logging is applied in two of three components

Indexer and enrichment worker put variables in fields; the API interpolates them
into the message. Grouping works on the first two and not on the third.

### I5. Log retention holds three values

Lambda groups 30 days, ECS 90 days, three CDK custom-resource groups unset. Each
was chosen where it was written.

### I6. Alarms sit on four layers with no rule

Producer, transport, consumer and edge are all represented. Ingestion carries
three alarms, none of which moved during the 2026-07-29 event; the database host
carries none. The one alarm that behaves correctly reads the producer side, which
is also what makes it immune to a planned consumer pause.

### I7. Two error classifiers documented as mirrors differ on HTTP 429

Transient on the NFT path, permanent on the SEP-1 path. Ten `Mirrors X` doc
comments exist across `crates/`, each an invariant maintained by hand.

---

## The counter-pattern already in the repository

`libs/api-types` pairs a generated artifact with a CI gate:

- `extract-openapi` runs `cargo run -p api --bin extract_openapi` and writes
  `openapi.json`
- `generate` runs `openapi-ts` over it into `src/generated`
- `check-generated` runs `git diff --exit-code` over both paths

A change on the Rust side that is not reflected in the committed generated output
fails CI. The contract is not maintained by hand, and the gate is a diff rather
than a test anyone has to write.

The same shape applies to the pairs in I1–I4, in ascending cost:

- **Assert, no generation.** A test reading both sides and comparing — for
  example, every `filterPattern` literal and metric namespace in the CDK must
  appear somewhere under `crates/`. Cheapest, and covers I2 entirely.
- **Reference instead of restate.** Where both sides are TypeScript, pass the
  construct and read `.queueName` rather than retyping the literal. Removes the
  pair rather than watching it.
- **Generate.** Where a vocabulary is genuinely shared, emit one side from the
  other, as `api-types` does.

---

## Steps

Ordered by confidence. Nothing here is started.

### A — Record keeping

- **A1** Task README reconciled with the measurements — _done, `cd965acd`_
- **A2** Measurement note committed — _done, `cd965acd`_
- **A3** This note committed
- **A4** Decide what happens to the parked alarm work and the draft ADR: both
  predate these measurements and the alarm work assumed the filter string would
  be substituted rather than replaced by a published counter
- **A5** Decide whether the coverage matrix belongs in `docs/architecture/` as a
  standing checklist rather than in a task note

### B — Changes needing no new infrastructure

- **B1** `RUST_LOG` on the API Lambda
- **B2** `SOROBAN_RPC_URLS` on the API Lambda
- **B3** Test under `infra/` asserting every filter string and metric namespace
  appears in `crates/` — covers I2 and any later instance
- **B4** Replace restated queue and cluster name literals with construct
  references
- **B5** Raise the 5xx threshold and add a minimum-request guard
- **B6** Move variables out of API message text into fields — makes I4 uniform
  and makes grouping work on the third component

### C — Changes needing a deploy

- **C1** Failed reconcile reaches the Lambda error metric, so the two alarms
  currently unable to move start working without being touched
- **C2** Indexer publishes a failure counter; the alarm reads the counter instead
  of message text
- **C3** Alarm on ingest queue age, gated on the queue being polled so a declared
  pause does not trip it
- **C4** DLQ alarms read growth rather than level, so they re-arm
- **C5** Clear the standing DLQ contents so the latched alarms return to OK
- **C6** Activate the cost allocation tag — not retroactive
- **C7** Align dashboard widgets with the alarm set, and correct the two widgets
  that read a retired signal and an unprovisioned feature

### D — Decisions rather than code

- **D1** X-Ray: the sampling rule duplicates the AWS default rule exactly, and
  audit history for 90 days records no trace read. Tracing is named as a
  delivered artefact in milestone documentation, so its status is a decision
- **D2** The origin-lock canary, gated false in the only deployed environment
- **D3** Whether "component publishes: alive / progressed / failed with kind"
  becomes a stated rule
- **D4** Whether a written convention covers identifiers in a public repository
- **D5** Log retention: one rule, or per-component with a stated reason

### E — Code defects found in passing

- **E1** HTTP 429 classified transient on one path and permanent on the other
- **E2** The audit projection differs from the struct it mirrors and reports
  parity regardless
- **E3** The sentinel row carries no reason, and the live worker discards
  `EnrichOutcome`
- **E4** SEP-1 documents capped at 100 KiB against 256 KiB on the NFT path

---

## Unreferenced or unreachable

Listed as observations. Each is small; none is proposed for removal here.

- `ObservabilityStack` contains one X-Ray sampling rule whose parameters match
  the AWS default rule it precedes, so it does not alter sampling.
- `HandlerError::ClickHouse`'s Display string is not reachable: every path logs
  the sanitised label instead. A CDK comment describes filter behaviour that
  depends on it.
- The origin-lock canary and its alarm are behind a flag set false in the only
  deployed environment.
- The dashboard's cache widget reads a cache left unprovisioned.
- `deploy-staging.yml` exists while `docs/deployment.md` records the staging path
  as retired.
- 18 files carry `CLICKHOUSE_URL`-gated tests; no pipeline sets the variable, so
  they skip and report green (task 0406).
- Three CDK custom-resource log groups and one API Gateway welcome group hold no
  data and no retention setting.
- `assets_pre0339` exists on the deployed database and not in `init.sql`
  (task 0400).

---

## Session record

For anyone picking this up later, including which conclusions were revised.

**Starting point.** Sixteen open tasks describing monitoring gaps, and a question
about whether they are sixteen problems or fewer.

**Method that worked.** Every statement checked against the production account or
database before being recorded. Three conclusions were revised this way, each
after the measurement contradicted the reading.

**Revised: three multi-hour lag events read as unnoticed outages.** The audit log
shows them as declared maintenance pauses. The first query used an event name
without its API version suffix and returned empty, which read as absence of
pauses rather than absence of matching records.

**Revised: a single periodic comparator as the fix for declared-vs-actual.** The
infrastructure comparison already runs, its output was read on two occasions, and
the delta stayed open across both. The gap is not detection.

**Revised: an enrichment regression affecting ~59 000 assets.** The blank rate did
move from 7.7 % to 99.7 %. Fetching 16 issuer documents directly found no case
where metadata is published upstream and the stored row is blank. The series
tracks the population being processed. A yield metric would have breached on it.

**Revised: enrichment failures produce no signal.** ~5 500 structured records a
week carry a stable reason field and group correctly. The gap is a reader.

**Held up.** The dead filter string, measured from its own metric. The API
logging level, measured from cold-start counts. Latching, measured from alarm
history. Notification volume concentrated in one alarm. Documentation describing
alarms not present in the account.

**Emerged late.** Reading the dashboard and metric routes alongside the alarms
showed the scope is four surfaces rather than one, and that the metric column is
largely already populated.
