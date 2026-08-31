---
prefix: R
title: Cross-check of our alarm set against CloudWatch's recommended alarms
status: mature
---

# R — AWS recommended alarms vs our set (ADR 0054 gate)

Source: the official CloudWatch "recommended alarms" catalogue
(`docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/Best_Practice_Recommended_Alarms_AWS_Services.html`),
fetched 2026-08-11 for Lambda / ECS / API Gateway / CloudFront; the SQS and
SNS sections truncated in the fetch and are reproduced from knowledge of the
same catalogue — marked (k). Rule for this note (the gate's wording): every
recommendation we lack gets a **deliberate yes/no**, never silence.

Our deployed-or-in-code set at the time of the check: `galexie-ingestion-lag`
(SQS Sent < 1, BREACHING), `ingestion-backlog-age` (SQS oldest-age > 120 s),
2× DLQ depth (level, zero-steady-state policy), 2× Lambda error-rate
(indexer, worker), `indexer-ch-write-failures` (log metric filter),
`api-gateway-5xx` (count > 0), `galexie-ephemeral-storage` (% ratio),
origin-lock canary (flag off), Cost Anomaly Detection.

## Verdicts

| AWS recommends                                                     | Our verdict                                       | Why                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **SQS** ApproximateAgeOfOldestMessage (k)                          | **HAVE**                                          | `ingestion-backlog-age`, threshold from measured distribution                                                                                                                                                                                                                                                         |
| **SQS** NumberOfMessagesSent low (k)                               | **HAVE**                                          | `galexie-ingestion-lag` is exactly this pattern, BREACHING on silence                                                                                                                                                                                                                                                 |
| **SQS** ApproximateNumberOfMessagesVisible (k)                     | HAVE for DLQs; **no** for main queues             | age is the SLO-shaped signal for a work queue; depth without age is ambiguous (a fast consumer can hold high depth harmlessly). DLQs use depth because their steady state is zero                                                                                                                                     |
| **SQS** ApproximateNumberOfMessagesNotVisible / inflight limit (k) | **no**                                            | inflight ≤ concurrency(1) × batch(≤10) — five orders of magnitude under the 120k limit by construction                                                                                                                                                                                                                |
| **Lambda** Errors                                                  | **HAVE** (indexer, worker) / covered (API)        | API Lambda crashes surface as gateway 5xx, and the 5xx alarm pages on a single one — a second alarm would double-page (ADR rule 3)                                                                                                                                                                                    |
| **Lambda** Throttles > 0                                           | **no**                                            | throttles are ROUTINE here: reserved concurrency 1 makes the SQS pollers throttle by design (documented at the ingest queue's maxReceiveCount: 10). An alarm would page on normal operation                                                                                                                           |
| **Lambda** Duration ≈ 80% of timeout                               | **no**                                            | indexer bounds itself (RECONCILE_DEADLINE 540 s of a 600 s timeout — by design, every catchup run approaches it); API timeout surfaces as 504 → 5xx alarm; latency is a dashboard concern (p50/95/99 widget)                                                                                                          |
| **Lambda** ConcurrentExecutions ≈ 80% of account limit             | **no**                                            | three functions, two pinned at concurrency 1, low-traffic API — the account limit is unreachable by construction                                                                                                                                                                                                      |
| **ECS/CI** EphemeralStorageUtilized ≈ 90%                          | **HAVE, stricter**                                | ours is a % ratio at 60 with a measured act-before-ceiling rationale (2026-07-01 deadlock)                                                                                                                                                                                                                            |
| **ECS/CI** RunningTaskCount ≤ 0                                    | **no**                                            | a stopped Galexie stops S3 writes → `galexie-ingestion-lag` (BREACHING) fires in 5 min; a task-count alarm would be a second witness to the same absence (ADR rule 3)                                                                                                                                                 |
| **ECS** CPU/Memory utilization ≈ 80%                               | **no**                                            | generously sized (13 GiB task); a wedged/OOM task stops output → lag alarm. Candidate dashboard widgets at most — noted for C7                                                                                                                                                                                        |
| **APIGW** 5XXError ≥ 5%                                            | **HAVE, stricter**                                | count > 0 (zero-tolerance; measured base rate 0/24 d) — strictly tighter than the 5% ratio                                                                                                                                                                                                                            |
| **APIGW** 4XXError ≥ 5%                                            | **no**                                            | 4xx on a public explorer with an auth layer is routine client traffic (401s, bad inputs). Dashboard widget exists; paging on it trains mute                                                                                                                                                                           |
| **APIGW** Latency p90 > 2.5 s                                      | **no**                                            | dashboarded; a latency regression is not a page for a two-person team, and a hard stall becomes 504 → 5xx alarm                                                                                                                                                                                                       |
| **APIGW** Count drop below baseline                                | **no**                                            | low-traffic site: zero-request windows are normal (74% of 5-min windows), so a baseline alarm is noise by construction. The honest answer to "is the edge up" is a synthetics canary — deliberately not deployed (M3 report records uptime as derived, not probed); revisit with the origin-lock canary decision (D2) |
| **SNS** NumberOfNotificationsFailed > 0 (k)                        | **no, folded into the delivery-witness decision** | on the ALARM topic this is self-referential (its own notification rides the same topic) — it is exactly the "chain has no witness" problem recorded as an open decision in ADR 0054; on the ledger fan-out topic a failed SQS delivery starves the queue → `galexie-ingestion-lag` fires (Sent drops to 0)            |
| **CloudFront** 5xxErrorRate / OriginLatency                        | **no**                                            | static SPA from S3; frontend telemetry is explicitly out of the umbrella's scope (task Notes). A broken edge is the same synthetics-canary question as above (D2)                                                                                                                                                     |
| **CloudFront** Function errors/throttles                           | n/a                                               | no CloudFront Functions deployed                                                                                                                                                                                                                                                                                      |
| **S3** replication/error metrics                                   | n/a / **no**                                      | no replication; bucket-level failures surface immediately in Galexie (writes) and the indexer (reads), both alarmed                                                                                                                                                                                                   |

## Conclusion

No blind adoption needed: every recommendation is either already covered
(often in a stricter, measured form), redundant with a BREACHING witness we
already pay for (rule 3), routine-by-design here (throttles), or a
deliberate no with the reason above. Two forward pointers fell out: ECS
CPU/memory as C7 dashboard candidates, and the synthetics-canary question
attached to the D2 decision. This closes the ADR 0054 gate "cross-checked
against CloudWatch's recommended alarms".
