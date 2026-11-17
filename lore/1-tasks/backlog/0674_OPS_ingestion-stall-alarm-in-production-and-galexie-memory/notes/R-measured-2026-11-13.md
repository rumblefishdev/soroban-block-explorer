---
title: 'Measured 2026-11-13: ingestion alarms and Galexie memory'
type: research
status: mature
spawned_from: null
spawns: []
tags: ['monitoring', 'cloudwatch']
links: []
history:
  - date: '2026-11-13'
    status: mature
    who: stkrolikiewicz
    note: 'Read-only CloudWatch (profile soroban-readonly, eu-central-1)'
---

# Measured 2026-11-13: ingestion alarms and Galexie memory

All calls are read-only `describe-*` / `get-*`. Times are +01:00 unless
marked.

## Alarm state

`aws cloudwatch describe-alarms --alarm-names ...`:

| Alarm                              | Metric                                 | Shape                  | Missing      |
| ---------------------------------- | -------------------------------------- | ---------------------- | ------------ |
| `production-galexie-ingestion-lag` | AWS/SQS NumberOfMessagesSent Sum 300 s | < 1, 1 period          | breaching    |
| `*-ingestion-backlog-age`          | AWS/SQS ApproximateAgeOfOldestMessage  | Max 60 s > 120, 3 of 3 | notBreaching |
| `testnet-ingestion-stall`          | IngestionLagSeconds Max 60 s           | > 60, 3 of 3           | breaching    |
| `production-ingestion-stall`       | does not exist                         |                        |              |

The account has 155 metric alarms. None of the explorer's alarms reads
AWS/ECS or memory. The only memory alarm is `x402-production-memory`.

## Pages, 2026-10-14 .. 11-13

From `describe-alarm-history --history-item-type StateUpdate`, counting the
"to ALARM" transitions:

- `production-galexie-ingestion-lag`, 8: 10-17 02:15, 10-21 13:34,
  10-28 22:06, 22:44, 11-06 01:21, 14:36, 15:26, 15:36.
- `production-ingestion-backlog-age`, 8: 10-17 02:53, 10-21 14:20,
  10-28 22:32, 11-06 01:54, 13:49, 15:13, 15:34, 17:01.
- `testnet-ingestion-backlog-age`, 5: 10-15 09:55, 11-03 06:08,
  11-06 13:49, 17:01, 11-12 07:23.
- `testnet-ingestion-stall`, 1: 10-15 09:58.

From 10-29 to 11-13, production got 4 + 5 = 9 pages.

## Replay of the stall shape

The data is `get-metric-statistics IngestionLagSeconds Environment=<env>
--period 60 --statistics Maximum`, in 23-hour chunks from 10-29 16:00 to
11-13 14:59: 21,448 points for production and 21,535 for testnet.

The rule: ALARM when the last 3 minutes are all > 60 s or missing; OK after 3
minutes in range.

- Production, 2 incidents: 11-06 01:16–01:58 and 11-06 14:31–15:49.
- Testnet, 1: 11-09 00:40–00:43. The real alarm did not fire then. Real
  CloudWatch evaluates late datapoints, which this replay counts as missing,
  so the replay is stricter than the alarm.

Production daily max of IngestionLagSeconds (s): 10-29 23, 10-30 7, 10-31 26,
11-01 7, 11-02 30, 11-03 8, 11-04 30, 11-05 7, **11-06 2315**, 11-07 12,
11-08 7, 11-09 21, 11-10 20, 11-11 24, 11-12 7, 11-13 25.

## Why backlog-age pages without an outage

- Ingest queue: VisibilityTimeout 660 s, maxReceiveCount 10.
- Indexer: `get-function-concurrency` gives 1. The event-source mapping has
  batch 1 and `ReportBatchItemFailures`.

A throttled receive hides the doorbell for 660 s. The oldest message's age
then climbs linearly to about 640 s and resets, while lag stays low.

## Galexie memory

Source: AWS/ECS `MemoryUtilization`, ClusterName=production-ingestion,
ServiceName=production-galexie-live, daily Maximum (%).

- 08-01..08-12: 90–92. 08-13: 80 (restart), then climbing to 89–90 by 08-24.
- 09-03..09-10: 92–94. 09-15: 77 (restart). 09-18..10-02: 94–97 (peak 97.0
  on 09-18 02:00 +02:00). 10-03: 78 (restart).
- 10-14..11-13: between 62 and 83. Drops on 10-17, 10-22 and 11-06; 64.6 on
  11-13.

Container Insights `MemoryUtilized` / `MemoryReserved` agree: daily max
8,287–11,057 MiB of 13,312 MiB over the last 30 days.

Hourly Maximum from 08-01 to 11-13 (2,510 hours), replaying "> T% for 3
consecutive hours" with no restarts assumed:

| T   | Pages | First hours (UTC)                                | Hours above |
| --- | ----- | ------------------------------------------------ | ----------- |
| 85% | 5     | 08-01 04, 08-17 17, 08-29 16, 09-11 23, 09-17 14 | 1,225       |
| 90% | 4     | 08-05 23, 09-03 11, 09-14 07, 09-18 13           | 682         |
| 95% | 1     | 09-18 13                                         | 138         |

`aws ecs describe-services ... events` keeps only recent stops and starts
(11-06 01:13 and 14:28). The stop reasons are not kept, so whether a restart
was an OOM kill is unknown.
