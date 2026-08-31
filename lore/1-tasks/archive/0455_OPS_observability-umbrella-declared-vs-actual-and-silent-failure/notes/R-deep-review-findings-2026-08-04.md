---
prefix: R
title: Observability surface — measured state across logs, metrics, dashboard and alarms
status: mature
---

# R — Measured state of the observability surface (2026-08-04)

A survey of what exists today across all four surfaces, with every claim checked
against the production account or database. Findings that did not survive that
check are recorded as refuted; the refutations carry the same weight as the
confirmations.

## How to read this

- **CONFIRMED** — checked by direct query against the live account or database.
- **REFUTED** — proposed, then contradicted by measurement.
- **NARROWED** — the original statement was broader than the evidence supports.

No entry rests on a code reading alone.

---

## Shape of the surface

The four surfaces were added at different times, each alongside the feature that
needed it. Read together they do not form a single design, and the gaps between
them are where the confirmed findings sit.

| Surface   | Current state                                                                                                                                                                                  |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Logs      | Three Lambdas, one JSON subscriber each. Two have `RUST_LOG` set; one does not. Two emit variables as structured fields; one interpolates them into the message text.                          |
| Metrics   | Three routes into CloudWatch exist: AWS-native, custom `put_metric_data`, and a log metric filter. One component uses all three, one uses one, one uses none of its own.                       |
| Dashboard | 11 widgets. Seven have no alarm; two alarms have no widget; one widget measures a question an alarm also answers, using a different signal; one widget reads a feature disabled in production. |
| Alarms    | Eight deployed across four layers (producer, transport, consumer, edge), with no stated rule for which layer a given condition belongs to.                                                     |

### Route asymmetry between the three Lambdas

| Component         | `RUST_LOG` | Own metrics | Log metric filter | Alarm on own metrics                 |
| ----------------- | ---------- | ----------- | ----------------- | ------------------------------------ |
| Indexer           | set        | yes         | yes               | 2                                    |
| Enrichment worker | set        | no          | no                | 2                                    |
| API               | not set    | no          | no                | none (its alarm sits on the gateway) |

`put_metric_data` appears in one crate. Exactly one `logs.MetricFilter` exists in
the whole CDK app.

### Dashboard versus alarms

- Widgets with no corresponding alarm: duration percentiles (both Lambdas),
  last processed ledger, concurrent executions, cold starts, cache hit/miss,
  4xx count.
- Alarms with no corresponding widget: Galexie ephemeral storage, enrichment
  worker error rate.
- `Galexie S3 freshness` graphs Lambda invocations; the alarm answering the same
  question reads SQS `NumberOfMessagesSent`. Task 0367 moved the alarm off the
  invocation signal because a batching consumer makes it unreliable; the widget
  still uses it.
- `API Gateway cache hit / miss` reads a cache that `apiGatewayCacheEnabled:
false` leaves unprovisioned in production.
- `IngestionLagSeconds` is published by the indexer and appears on neither the
  dashboard nor any alarm.
- `Last processed ledger sequence` is graphed with no reference series to
  compare against.

### Log retention

Three values, set independently: Lambda groups 30 days, ECS 90 days
(`ecsLogRetentionDays`), and three CDK custom-resource groups with no retention
set. The 30-day figure bounds any investigation of an event older than a month.

---

## The recurring shape

Each confirmed finding below is an instance of one shape:

> A value is written in one artifact and restated by hand in another, with no
> mechanism comparing the two.

Instances: alarm filter text ↔ log message text; an audit struct ↔ the struct it
projects; TypeScript enum maps ↔ Rust enums; environment variables set for two of
three Lambdas; roughly 55 doc comments describing parity with a Postgres
implementation not present in this repository; and row existence standing in for
a failure reason.

A dependency graph over 592 files reports no cycles, correct dependency
direction, and no oversized module. The distance between the two sides of each
pair above crosses a language or artifact boundary, which is outside what a
single-language tool inspects.

`libs/api-types` already implements the counter-pattern for one of these pairs:
a generated type plus a CI `git diff --exit-code` gate, so a Rust-side change
that is not reflected on the TypeScript side fails the build.

---

## CONFIRMED

### 1. The `ChWriteFailures` metric filter matches a string the code no longer emits

The deployed filter matches `failed to process S3 record`. That string was
removed by `bee784df` (2026-05-27) when the handler moved to the reconcile loop;
the terminal failure line is now `reconcile failed — will redeliver doorbell`
(`crates/indexer/src/handler/mod.rs:174`). The surviving pattern in the filter,
`failed to build mTLS CH client`, is emitted only at cold start, and the alarm
threshold (`> 10` in 5 minutes) is above what that path produces.

Measured: the filter evaluates ~108 000 log events per day and its metric has
been 0.0 for every bucket across 60 days, including every 5-minute window of the
2026-07-29 ingestion outage.

Substituting the current string would restore the alarm and reproduce the
coupling. An alternative is a counter published deliberately by the indexer on
the failure path, which removes the dependency on message wording.

### 2. The API Lambda has no `RUST_LOG`, so its subscriber filters at ERROR

`EnvFilter::from_default_env()` defaults to `LevelFilter::ERROR` when the
variable is unset. It is set for the indexer and the enrichment worker.

Measured: 470 cold starts in 7 days and zero `api cold start` lines, an `info!`
that fires unconditionally after subscriber init. Eleven `warn!` sites are
filtered out, each on a path that returns HTTP 200 with a partial result — empty
participant arrays on transaction detail, a dropped ETag, `metadata: null` on
NFTs, a missing asset description, a truncated assets page.

Archived task 0113 records the same condition for the indexer, where it was
resolved. The API Lambda was not in that scope.

Note on effect: setting the variable makes those messages available; no alarm or
scheduled query currently reads them. The enrichment worker has the variable set
and emits structured warnings, and no reader consumes those either.

### 3. `SOROBAN_RPC_URLS` is set for the enrichment worker and not for the API

The API therefore resolves to a single default endpoint rather than the
configured pool.

### 4. `IngestionLagSeconds` is published after the persist call

`persist_with_retry(...).await?` returns early on failure, so the publish on the
following line does not run. A stalled indexer produces no lag datapoints rather
than rising ones. No widget or alarm in `infra/` references the metric.

### 5. Notification volume concentrates in one alarm

`api-gateway-5xx-rate` recorded 32 state transitions in one month. On one day 5
errors produced 8 transitions; on another, 43 errors produced 16. All returned to
OK within minutes.

24 of 32 transitions originated from 48 errors in total. The configuration is a
0.5 % threshold, one evaluation period, and no minimum-request guard. At 5 %
sustained over 15 minutes, the 2026-07-07 event (80 % error rate) still breaches
and the low-volume transitions do not.

### 6. A latched alarm produces no further notifications

The enrichment DLQ reached 11 776 messages and its alarm recorded zero
transitions across that period, having entered ALARM beforehand. Depth returned
to single digits 14 days after the peak, matching `MessageRetentionPeriod`. The
ledger-processor DLQ alarm followed the same pattern over 15 days.

CloudWatch notifies on state transition, so an alarm already in ALARM neither
repeats nor signals a subsequent event.

### 7. Architecture docs describe alarms not present in the account

`docs/architecture/technical-design-general-overview.md` lists a ClickHouse CPU
alarm and a free-disk alarm. The database-host alarms were removed in task 0239.
No alarm in the account references that host. Two files describe the
ingestion-lag alarm as firing on S3 timestamps 60 s behind ledger close; the
deployed alarm counts queue messages over a 5-minute window.

Of 14 commits touching alarm or monitoring shape, 9 contain no change under
`docs/architecture/`, which ADR 0032 asks for in the same change.

### 8. `infra/` has no test target

Eleven stacks, eight alarms and every Lambda environment are typechecked and
linted. No assertion covers the values that must match a value elsewhere.
Findings 1, 2 and 3 are instances.

The DLQ alarms take queue construct objects; the Galexie alarm restates the same
queue name as a string literal. The first form removes the coupling.

### 9. The audit projection differs from the struct it mirrors

`crates/audit-harness/src/bin/operations-order-diff.rs` documents lockstep with a
path that no longer exists, references a constraint name absent from `crates/`,
and differs from `crates/db-clickhouse/src/persist/stage.rs` by one field type
and one extraction block. Pool identity is therefore under-projected while the
comparison reports parity.

### 10. Two retry classifiers documented as mirrors differ on HTTP 429

`nft_token_uri::errors::is_transient` treats 429 as transient;
`sep1_assets::is_transient` treats it as permanent, so a rate-limited issuer
receives a sentinel row and is excluded from the candidate query.

Ten `Mirrors X` doc comments exist across `crates/`, each describing an invariant
maintained by hand without a corresponding test.

---

## REFUTED

### R1. A regression in asset enrichment success

Blank-rate on assets whose issuer has a `home_domain` moves from 7.7 % (June) to
99.7 % (July) and holds at ~99 % through August. The daily series shows a step
between 2026-06-21 and 2026-06-29 and sustains at low live volume, so the July
backfill does not explain it.

Sixteen issuer domains were sampled from the blank population and fetched
directly: three do not resolve, one returns a document with zero `CURRENCIES`
entries, and nine are subdomains of one project behind a Cloudflare block page.
No case was found where a document contains the asset's metadata and the row is
blank.

The step reflects a change in the population being processed — from assets with
published metadata to a long tail without — rather than a change in behaviour.
The blank rows are the expected result.

A derived observation: an "enrichment yield" alarm, had one existed, would have
breached on this series. A metric with no notion of whether metadata is
published upstream measures the population rather than the pipeline.

### R2. Enrichment failures produce no signal

The worker emits ~5 500 structured `WARN` records per week carrying a stable
`reason` field, which group correctly under aggregation. A code comment records
that an earlier silent path was changed to log per key.

The gap is a reader, not a signal. This also refutes the related statement that
the worker's records cannot be grouped.

### R3. A failed fetch writes no row, so the producer republishes the key indefinitely

Permanent failures write a row via `permanent_fail_outcome`. Only transient
failures write nothing, and 9 occurred in 7 days. The enrichment queue holds no
messages.

---

## NARROWED

### N1. The schema offers no state for permanent failure

The state exists and is named:

```rust
/// Both variants INSERT a side-table row (existence = "tried"); the split is
/// what the backfill report and the `status` query distinguish — they must not
/// be lumped (the report used to fold both into one "succeeded" count).
pub enum EnrichOutcome { Real, Sentinel }
```

Two narrower observations hold:

- The live worker discards the value (`...await?; Ok(())`), so the split is
  consumed only by the backfill report.
- The sentinel row carries no reason, so "no metadata published", "fetch blocked
  upstream" and "document above the size cap" are the same row. Establishing R1
  above required fetching 16 documents by hand for this reason.

A related pair of values: `crates/enrichment-shared/src/sep1/client.rs:46` caps
fetched documents at 100 KiB and the NFT path uses 256 KiB
(`nft_token_uri/client.rs:29`). Neither is configurable or surfaced.

---

## Candidate work, ordered by breadth of effect

| #   | Change                                                                                          | Scope covered                                           |
| --- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| 1   | A test in `infra/` asserting that every filter string and metric namespace appears in `crates/` | Findings 1, 2, 3 and future instances of the same shape |
| 2   | Queue and cluster construct references in place of restated name literals                       | Removes three couplings                                 |
| 3   | `RUST_LOG` and `SOROBAN_RPC_URLS` on the API Lambda                                             | Findings 2 and 3                                        |
| 4   | A reason on the sentinel row; retain `EnrichOutcome` in the worker                              | N1                                                      |
| 5   | Reduce the alarm set; raise the 5xx threshold and add a volume guard                            | Findings 5 and 6                                        |
| 6   | Align dashboard widgets with the alarm set                                                      | Dashboard section above                                 |

Measured candidates for removal: the X-Ray sampling rule is parameter-identical
to the AWS default rule it precedes, so removing it leaves sampling unchanged.
X-Ray traces contain the Lambda envelope only, and audit history for 90 days
records no `GetTraceSummaries` or `BatchGetTraces` call; tracing is named as a
delivered artefact in milestone documentation, so its status is a decision rather
than a cleanup. The origin-lock canary is gated on a flag set false in the only
deployed environment.

Two alarms read Lambda `Errors`, which summed to 0 over 30 days containing seven
lag events, because the handler returns success with a batch-item failure.

---

## Open

- The 100 KiB cap: no affected issuer appeared in the sample, so its effect is
  unquantified.
- Whether documents behind the Cloudflare block page are reachable from Lambda. A
  browser user-agent did not change the response.
- The account identifier appears in several files and in the queue URL logged at
  cold start. A repository-wide search finds no written convention covering this,
  so the convention would need stating before any cleanup is durable.
