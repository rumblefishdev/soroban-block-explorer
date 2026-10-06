import * as cdk from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import type * as ecs from 'aws-cdk-lib/aws-ecs';
import type * as sqs from 'aws-cdk-lib/aws-sqs';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';
import { addGalexieProtocolWatch } from './galexie-protocol-watch.js';

export interface IngestionAlarmsProps {
  readonly config: EnvironmentConfig;
  readonly ingestQueue: sqs.IQueue;
  readonly galexieCluster?: ecs.ICluster;
  readonly galexieService?: ecs.IBaseService;
  /** Attaches the stack's alarm and OK actions (Slack). */
  readonly withActions: (alarm: cloudwatch.Alarm) => cloudwatch.Alarm;
}

/**
 * Alarms on getting ledgers in: is our Galexie producing them (1, 1b) and is
 * a Galexie with a newer core out (1d) or, with the public data lake, is the
 * newest indexed ledger young (1c); and is the indexer consuming them (1a).
 * Part of `CloudWatchStack`; created in its scope, so the alarms keep their
 * construct ids.
 */
export function addIngestionAlarms(
  scope: Construct,
  {
    config,
    ingestQueue,
    galexieCluster,
    galexieService,
    withActions,
  }: IngestionAlarmsProps
): void {
  // Alarms 1 and 1b watch our own Galexie; with `public-lake` there is
  // none (task 0553).
  if (galexieCluster && galexieService) {
    // ---------------------
    // Alarm 1: Galexie ingestion lag
    // Fires when NO new ledger has landed in S3 for `galexieLagMinutes` — i.e.
    // the S3 → SNS → SQS doorbell rate on the ingest queue dropped to 0.
    //
    // Why this signal (SQS NumberOfMessagesSent) and not Lambda Invocations:
    // the indexer's reconcile drains a contiguous backlog for up to 9 min per
    // invocation (RECONCILE_DEADLINE = 540 s), so invocation STARTS can be ~9
    // min apart even when healthy (any catchup/backlog burst). An
    // invocation-based window therefore can't drop below ~10 min without
    // false-firing. The doorbell rate tracks Galexie's ACTUAL output — one S3
    // object (→ one SNS→SQS message) per ledger close, ~every 5-6 s —
    // regardless of how the indexer batches, so a 5-min window is both safe and
    // fast: 5 min with zero new objects ≈ 50 missed writes = Galexie stopped.
    // A deliberate indexer pause (concurrency 0) does NOT trip this — doorbells
    // still land in the queue; that is the point of measuring the input, not
    // the consumer.
    //
    // treatMissingData: BREACHING is REQUIRED, not cosmetic. SQS (like Lambda)
    // publishes no datapoint when idle — a true stop makes the metric go
    // ABSENT, not 0. Under NOT_BREACHING that absence reads as healthy and the
    // alarm can NEVER fire on the one condition it exists to catch. That bit us
    // 2026-07-08: Galexie stalled ~16 h on the pubnet proto-27 upgrade and the
    // old NOT_BREACHING invocations alarm stayed green the whole time (see
    // lore-0367). BREACHING makes "no data" = alarm. Do NOT revert.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(scope, 'GalexieLagAlarm', {
        alarmName: `${config.envName}-galexie-ingestion-lag`,
        alarmDescription:
          'No new ledgers landed in S3 (0 doorbells to the ingest queue) for the lag window - Galexie may have stopped writing.',
        metric: new cloudwatch.Metric({
          namespace: 'AWS/SQS',
          metricName: 'NumberOfMessagesSent',
          dimensionsMap: { QueueName: ingestQueue.queueName },
          period: cdk.Duration.minutes(config.galexieLagMinutes),
          statistic: cloudwatch.Stats.SUM,
        }),
        threshold: 1,
        comparisonOperator: cloudwatch.ComparisonOperator.LESS_THAN_THRESHOLD,
        evaluationPeriods: 1,
        treatMissingData: cloudwatch.TreatMissingData.BREACHING,
      })
    );

    // Alarm 1d: is a Galexie with a newer captive core out, ours not on it?
    addGalexieProtocolWatch(scope, {
      config,
      galexieCluster,
      galexieService,
      withActions,
    });
  }

  // ---------------------
  // Alarm 1a: ingest backlog age — the consumer-side counterpart to Alarm 1
  //
  // Alarm 1 watches the PRODUCER (are ledgers landing in S3). This one
  // watches whether they are being CONSUMED: the age of the oldest queued
  // doorbell. The 2026-07-29 outage (lore-0454) sat exactly in that gap —
  // Galexie kept delivering, the indexer persisted nothing for 19 minutes,
  // all seven alarms stayed green, and this metric tracked it perfectly
  // (0 → 1421 s) with nothing reading it.
  //
  // Deliberately a BARE threshold: one knowing page per planned pause is
  // the accepted cost (ADR 0054 rule 4, which also records the measured
  // `IF(received > 0, age, 0)` discriminator and why it was withdrawn).
  //
  // Threshold and window are measured, not guessed (732 h to 2026-08-04):
  // the hourly max age had median 0 s / p90 1 s, and every hour above 60 s
  // is the same set as above 600 s — known incidents and declared pauses,
  // nothing in between. So any threshold in that band produces the same
  // page count; 120 s buys the earliest detection (0454 replay: pages
  // 09:43 vs 09:54 at 600 s, self-heal was 09:58). Three consecutive
  // minutes so a single stray datapoint cannot page anyone.
  //
  // NOT_BREACHING: an empty idle queue publishes no datapoint, and silence
  // of the producer is Alarm 1's job (BREACHING there) — paging both for
  // one fault is how alarms get muted (ADR 0054 rule 3).
  // ---------------------
  withActions(
    new cloudwatch.Alarm(scope, 'IngestBacklogAgeAlarm', {
      alarmName: `${config.envName}-ingestion-backlog-age`,
      alarmDescription:
        'Queued ledgers are not being consumed - oldest doorbell exceeded the age threshold. Real stall (lore-0454 shape) OR a paused/forgotten event-source mapping; if you just paused the indexer on purpose, this page is expected. Runbook: docs/deployment.md (pause procedure) + docs/runbooks/live-tail-cutover.md.',
      metric: new cloudwatch.Metric({
        namespace: 'AWS/SQS',
        metricName: 'ApproximateAgeOfOldestMessage',
        dimensionsMap: { QueueName: ingestQueue.queueName },
        period: cdk.Duration.minutes(1),
        statistic: cloudwatch.Stats.MAXIMUM,
      }),
      threshold: config.ingestionBacklogAgeSeconds,
      comparisonOperator: cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
      evaluationPeriods: 3,
      datapointsToAlarm: 3,
      treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
    })
  );

  if (galexieCluster && galexieService) {
    // ---------------------
    // Alarm 1b: Galexie ephemeral storage utilization (%)
    // captive-core's BucketList (current ledger state) + catchup temp live on
    // the task's ephemeral disk. Baseline ~30%; >60% sustained = plan a disk
    // bump BEFORE a merge/catchup spike hits the "No space left on device"
    // ceiling (the 2026-07-01/02 deadlock: full disk → catchup never completes
    // → temp never cleaned → task wedged while `pgrep stellar-core` still
    // reports healthy). Metric-math on % is robust to disk-size changes.
    // Sustained 3×5 min avoids paging on a transient merge spike.
    // Re-arm answer (rule 2, ADR 0054): a level alarm is correct here — the
    // condition is "act before the ceiling", and acting (disk bump / temp
    // cleanup, see the 0367 runbook trail) drops utilization below 60%,
    // which clears and re-arms the alarm. Standing >60% is never accepted.
    // ---------------------
    const ephemeralUsed = new cloudwatch.Metric({
      namespace: 'ECS/ContainerInsights',
      metricName: 'EphemeralStorageUtilized',
      dimensionsMap: {
        ClusterName: galexieCluster.clusterName,
        ServiceName: galexieService.serviceName,
      },
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.MAXIMUM,
    });
    const ephemeralReserved = new cloudwatch.Metric({
      namespace: 'ECS/ContainerInsights',
      metricName: 'EphemeralStorageReserved',
      dimensionsMap: {
        ClusterName: galexieCluster.clusterName,
        ServiceName: galexieService.serviceName,
      },
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.MAXIMUM,
    });
    withActions(
      new cloudwatch.Alarm(scope, 'GalexieEphemeralStorageAlarm', {
        alarmName: `${config.envName}-galexie-ephemeral-storage`,
        alarmDescription:
          'Galexie captive-core ephemeral disk >60% - approaching the deadlock ceiling; plan a disk bump.',
        metric: new cloudwatch.MathExpression({
          expression: '(used / reserved) * 100',
          usingMetrics: { used: ephemeralUsed, reserved: ephemeralReserved },
          period: cdk.Duration.minutes(5),
          label: 'Ephemeral Used (%)',
        }),
        threshold: config.galexieEphemeralUtilizationThreshold,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 3,
        datapointsToAlarm: 3,
        // NOT_BREACHING is correct here (task 0455 review): Container
        // Insights stops publishing when no task is running, so missing
        // data means "service stopped", not "disk full" — and a stopped
        // Galexie already pages via the lag alarm's BREACHING above.
        // Paging here too would double-page one incident.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );
  } else {
    // ---------------------
    // Alarm 1c: public-lake stall (task 0553)
    // Reading the public data lake, ingestion is healthy while the newest
    // indexed ledger stays young. One alarm covers a lake outage,
    // a testnet reset (the old genesis folder stops growing; the sequence
    // never goes backwards) and a protocol upgrade the parser cannot decode.
    // Simulated 2026-09-30 on an hour of measured testnet arrivals: with the
    // indexer pacing itself the lag peaks near 10 s, so 60 s for 3 minutes
    // never pages on a healthy lake. BREACHING: a stalled indexer publishes
    // no datapoint.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(scope, 'LakeStallAlarm', {
        alarmName: `${config.envName}-ingestion-stall`,
        alarmDescription:
          'The newest indexed ledger is over 60 s old for 3 minutes, or none was indexed. Lake outage, testnet reset or an undecodable protocol upgrade. Runbook: docs/runbooks/testnet-reset.md.',
        metric: new cloudwatch.Metric({
          namespace: 'SorobanBlockExplorer/Indexer',
          metricName: 'IngestionLagSeconds',
          dimensionsMap: { Environment: config.envName },
          period: cdk.Duration.minutes(1),
          statistic: cloudwatch.Stats.MAXIMUM,
        }),
        threshold: 60,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 3,
        datapointsToAlarm: 3,
        treatMissingData: cloudwatch.TreatMissingData.BREACHING,
      })
    );
  }
}
