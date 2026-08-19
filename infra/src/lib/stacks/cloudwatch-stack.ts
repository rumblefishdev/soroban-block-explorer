import * as cdk from 'aws-cdk-lib';
import * as ce from 'aws-cdk-lib/aws-ce';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import * as chatbot from 'aws-cdk-lib/aws-chatbot';
import * as cloudwatchActions from 'aws-cdk-lib/aws-cloudwatch-actions';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as sns from 'aws-cdk-lib/aws-sns';
import * as sqs from 'aws-cdk-lib/aws-sqs';
import * as apigateway from 'aws-cdk-lib/aws-apigateway';
import * as synthetics from 'aws-cdk-lib/aws-synthetics';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import type { Construct } from 'constructs';

import { originLockCanaryCode } from '../canaries/origin-lock.js';
import type { EnvironmentConfig } from '../types.js';

/**
 * ASCII ONLY in anything that reaches the synthesized template - alarm
 * descriptions, dashboard titles, markdown widgets.
 *
 * Not a style preference. `cdk diff` reads the deployed template back through
 * a path that mangles non-ASCII: byte-checked 2026-08-19, the live alarm
 * carries `e2 80 94` (an em dash) while the template read returns `3f` (`?`),
 * so every such string shows as a change that survives being deployed. Nine
 * alarm descriptions and one dashboard title were producing ten permanent
 * false entries in the diff - the diff an operator is asked to read before
 * every production deploy. A gate that always shows noise teaches people to
 * scroll past it, and that is what happened to the change that muted every
 * alarm for 19 hours on 2026-08-18.
 *
 * Comments are unaffected; they never reach the template. Use `-` for an em
 * dash and `->` for an arrow.
 */
export interface CloudWatchStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly apiFunction: lambda.IFunction;
  readonly processorFunction: lambda.IFunction;
  /** Ledger ingest queue — the Galexie lag alarm reads its doorbell rate. */
  readonly ingestQueue: sqs.IQueue;
  readonly deadLetterQueue: sqs.IQueue;
  /** Type-1 enrichment DLQ (task 0191) — alarmed on depth > 0. */
  readonly enrichmentDlq: sqs.IQueue;
  /** Type-1 enrichment worker Lambda (task 0191) — error-rate alarm. */
  readonly enrichmentWorkerFunction: lambda.IFunction;
  /** Galexie ECS cluster — ephemeral-storage alarm dimensions. */
  readonly galexieCluster: ecs.ICluster;
  /** Galexie live-ingest ECS service — ephemeral-storage alarm dimensions. */
  readonly galexieService: ecs.IBaseService;
  readonly restApi: apigateway.RestApi;
  /**
   * CloudFront `*.cloudfront.net` domain of the SPA distribution, used as a
   * target by the origin-lockdown canary (task 0277). Optional — when
   * absent the canary only checks the execute-api URL.
   */
  readonly spaDistributionDomainName?: string;
}

/**
 * Observability layer — CloudWatch dashboards and alarms.
 *
 * Creates:
 * - One SNS topic per environment for alarm notifications
 * - AWS Chatbot SlackChannelConfiguration subscribing the topic to a Slack channel
 * - 4 alarms covering Galexie ingestion lag, Processor error rate, DLQ depth
 *   and API Gateway 5xx rate
 * - A CloudWatch dashboard with Ingestion / API sections
 *
 * All alarm thresholds are env-configurable via EnvironmentConfig.
 *
 * Prerequisites (one-time manual step):
 * Authorize the Slack workspace in the AWS Console under AWS Chatbot before
 * deploying. Without this the SlackChannelConfiguration will fail to create.
 *
 * RDS alarms / widgets removed in task 0239 — the production data plane
 * lives on Hetzner ClickHouse, monitored separately on the box itself.
 */
export class CloudWatchStack extends cdk.Stack {
  readonly alarmTopic: sns.Topic;

  constructor(scope: Construct, id: string, props: CloudWatchStackProps) {
    super(scope, id, props);

    const {
      config,
      apiFunction,
      processorFunction,
      ingestQueue,
      deadLetterQueue,
      enrichmentDlq,
      enrichmentWorkerFunction,
      galexieCluster,
      galexieService,
      restApi,
      spaDistributionDomainName,
    } = props;

    // ---------------------
    // SNS Topic
    // ---------------------
    const alarmTopic = new sns.Topic(this, 'AlarmTopic', {
      topicName: `${config.envName}-soroban-explorer-alarms`,
      displayName: `${config.envName} Soroban Explorer Alarms`,
    });
    this.alarmTopic = alarmTopic;

    // ---------------------
    // AWS Chatbot — Slack channel
    // Workspace + channel IDs are deployment-specific identifiers we keep OUT
    // of the (public) repo, so they come from SSM Parameter Store (plain
    // String — not credentials) at deploy time, not from env config. Set once
    // out-of-band before deploy:
    //   aws ssm put-parameter --type String \
    //     --name /soroban-explorer/${envName}/slack-workspace-id --value T...
    //   aws ssm put-parameter --type String \
    //     --name /soroban-explorer/${envName}/slack-channel-id   --value C...
    // Prerequisites (one-time, manual): authorize the Slack workspace in the
    // AWS Console under AWS Chatbot, and `/invite @aws` in the target channel.
    // ---------------------
    const slackWorkspaceId = ssm.StringParameter.valueForStringParameter(
      this,
      `/soroban-explorer/${config.envName}/slack-workspace-id`
    );
    const slackChannelId = ssm.StringParameter.valueForStringParameter(
      this,
      `/soroban-explorer/${config.envName}/slack-channel-id`
    );
    new chatbot.SlackChannelConfiguration(this, 'SlackChannel', {
      slackChannelConfigurationName: `${config.envName}-soroban-explorer-alarms`,
      slackWorkspaceId,
      slackChannelId,
      notificationTopics: [alarmTopic],
      role: new iam.Role(this, 'ChatbotRole', {
        assumedBy: new iam.ServicePrincipal('chatbot.amazonaws.com'),
        managedPolicies: [
          iam.ManagedPolicy.fromAwsManagedPolicyName(
            'CloudWatchReadOnlyAccess'
          ),
        ],
      }),
    });

    // ---------------------
    // Cost anomaly detection (task 0449 / 0455 defect 3)
    // The account had ZERO cost monitoring (measured 2026-08-10: no anomaly
    // monitor, no budget) — the July step change in one service's spend ran
    // for three weeks before a human read a bill. This monitor learns a
    // per-SERVICE baseline for the WHOLE account (tagged or not, current
    // services or future ones) and alerts with the root-cause service named,
    // so the next such step change is a same-day Slack message instead of a
    // month-end surprise. Free of charge; alerts ride the existing topic.
    //
    // Per-project budgets are the complement (creep vs spikes) and are added
    // separately once Fargate task tagging (propagateTags on the Galexie
    // service) has produced a week of honestly-attributed data — today the
    // Project tag sees ~9% of this project's real spend.
    // ---------------------
    // DANGER, read before touching this block. `addToResourcePolicy` on a
    // Topic synthesises an `AWS::SNS::TopicPolicy`, and that resource
    // REPLACES the topic's access policy outright — it does not merge with
    // the default policy SNS creates alongside a new topic. Adding the cost
    // grant alone therefore revoked the default owner statement that lets
    // CloudWatch publish, and every operational alarm went mute while still
    // evaluating and changing state perfectly: measured 2026-08-18 14:52 to
    // 2026-08-19 09:58, three `Failed to execute action` entries on this
    // topic while the co-tenant's topic delivered twelve in the same window.
    // Nothing noticed, because the delivery chain has no witness (ADR 0054,
    // open decision). So: every principal that must publish here is listed
    // EXPLICITLY below, and anything added later goes in the same list.
    // Same-account alarms publish AS THE ACCOUNT, so the owner statement is
    // the one that matters; a `cloudwatch.amazonaws.com` service-principal
    // grant is only needed for cross-account topics and was dropped as
    // redundant here.
    alarmTopic.addToResourcePolicy(
      new iam.PolicyStatement({
        sid: 'AllowOwnerAccountPublish',
        // The default statement SNS would have created, restored by hand:
        // CloudWatch alarms publish under the topic owner's account, so
        // without this every alarm action fails.
        principals: [new iam.AccountPrincipal(cdk.Stack.of(this).account)],
        actions: ['sns:Publish'],
        resources: [alarmTopic.topicArn],
      })
    );
    alarmTopic.addToResourcePolicy(
      new iam.PolicyStatement({
        sid: 'AllowCostAnomalyDetectionPublish',
        // Cost Anomaly Detection publishes from this service principal;
        // without the explicit topic-policy grant, subscription delivery
        // fails silently at deploy validation.
        principals: [new iam.ServicePrincipal('costalerts.amazonaws.com')],
        actions: ['sns:Publish'],
        resources: [alarmTopic.topicArn],
        // Confused-deputy guard: a service principal is not an identity, it
        // is "any caller reaching us through that service". Without this the
        // grant reads "Cost Anomaly Detection may publish here on behalf of
        // ANY account" — a stranger could point their monitor at our topic.
        // Scoping to our own account keeps the grant to our own anomalies.
        conditions: {
          StringEquals: { 'AWS:SourceAccount': cdk.Stack.of(this).account },
        },
      })
    );
    const costAnomalyMonitor = new ce.CfnAnomalyMonitor(
      this,
      'CostAnomalyMonitor',
      {
        monitorName: `${config.envName}-cost-anomaly-by-service`,
        monitorType: 'DIMENSIONAL',
        monitorDimension: 'SERVICE',
      }
    );
    new ce.CfnAnomalySubscription(this, 'CostAnomalySubscription', {
      subscriptionName: `${config.envName}-cost-anomaly-to-alarm-topic`,
      monitorArnList: [costAnomalyMonitor.attrMonitorArn],
      // IMMEDIATE = notify as soon as the anomaly is detected (cost data
      // refreshes a few times a day, so "immediate" means hours, not
      // minutes — still ~20x faster than the July discovery). SNS
      // subscribers require IMMEDIATE; DAILY/WEEKLY are email-only.
      frequency: 'IMMEDIATE',
      subscribers: [{ type: 'SNS', address: alarmTopic.topicArn }],
      // Only anomalies whose total impact reaches this many USD notify —
      // keeps single-cent blips out of Slack while the July shape (a
      // service's spend stepping up day after day) clears it easily.
      thresholdExpression: JSON.stringify({
        Dimensions: {
          Key: 'ANOMALY_TOTAL_IMPACT_ABSOLUTE',
          MatchOptions: ['GREATER_THAN_OR_EQUAL'],
          Values: [String(config.costAnomalyAlertThresholdUsd)],
        },
      }),
    });

    const alarmAction = new cloudwatchActions.SnsAction(alarmTopic);

    // ---------------------
    // Helper: attach both alarm and ok actions
    // ---------------------
    const withActions = (alarm: cloudwatch.Alarm): cloudwatch.Alarm => {
      alarm.addAlarmAction(alarmAction);
      alarm.addOkAction(alarmAction);
      return alarm;
    };

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
    // the consumer (indexer health is covered by the alarms below).
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
      new cloudwatch.Alarm(this, 'GalexieLagAlarm', {
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
    // Deliberately a BARE threshold — no pause/failure discrimination. A
    // planned indexer pause (event-source-mapping disabled) WILL page once
    // when the backlog crosses the threshold; the operator who just paused it
    // knows exactly why, and that one knowing page also bounds the
    // forgot-to-re-enable case, which a discriminator would hide forever. An
    // `IF(received > 0, age, 0)` discriminator was designed, measured and
    // withdrawn as overcomplication — see ADR 0054, "Considered and
    // withdrawn".
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
      new cloudwatch.Alarm(this, 'IngestBacklogAgeAlarm', {
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
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 3,
        datapointsToAlarm: 3,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

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
      new cloudwatch.Alarm(this, 'GalexieEphemeralStorageAlarm', {
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

    // ---------------------
    // Alarm 2: Ledger Processor error rate
    // Uses a MathExpression: errors / invocations > threshold.
    // ---------------------
    const processorErrors = processorFunction.metricErrors({
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });
    const processorInvocations = processorFunction.metricInvocations({
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });
    withActions(
      new cloudwatch.Alarm(this, 'ProcessorErrorRateAlarm', {
        alarmName: `${config.envName}-ledger-processor-error-rate`,
        alarmDescription:
          'Ledger Processor error rate exceeded threshold - ledgers may be failing to index.',
        metric: new cloudwatch.MathExpression({
          expression: 'errors / invocations',
          usingMetrics: {
            errors: processorErrors,
            invocations: processorInvocations,
          },
          period: cdk.Duration.minutes(5),
          label: 'Error Rate',
        }),
        threshold: config.processorErrorRateThreshold,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): the ratio has
        // no datapoint when invocations are 0, and 0 invocations is not an
        // error-RATE problem — it is either a planned pause (must not page)
        // or a dead input, which is the lag alarm's job (BREACHING there).
        // Beware what this alarm can NOT see: a total stall never reaches
        // Lambda `Errors` at all — measured 0 through every lag event of a
        // 30-day window (0454). Absence coverage is the backlog-age alarm's
        // job (Alarm 1a), not this alarm's.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 2b: Indexer ClickHouse write / mTLS init failure (task 0241)
    // Counts the indexer's terminal failure log lines: a CH write that
    // failed AFTER the in-band retry envelope was exhausted, or an
    // mTLS bundle that could not be assembled at cold start. This is a
    // POST-RETRY hard-failure counter — a transient 5xx burst that the
    // retry envelope recovers from emits no matching line and does NOT
    // increment this metric (the per-retry "…hit transient CH error —
    // retrying" warn is intentionally not matched here). Complements
    // Alarm 2 (Lambda Errors metric): it pins the failure to the CH
    // write / mTLS path specifically, where the Errors rate alone
    // wouldn't tell you which subsystem broke.
    // ---------------------
    const chWriteFailureFilter = new logs.MetricFilter(
      this,
      'IndexerChWriteFailureFilter',
      {
        logGroup: logs.LogGroup.fromLogGroupName(
          this,
          'ProcessorLogGroupRef',
          `/aws/lambda/${processorFunction.functionName}`
        ),
        // JSON-anchored match on `$.fields.alarm` — a dedicated
        // machine-contract field, NOT the human `message` prose. The
        // indexer Lambda uses `tracing_subscriber::fmt().json()`, so
        // each log line is `{"fields":{"alarm":"...","message":...}}`.
        // History: this filter originally matched the prose `failed to
        // process S3 record`; the doorbell rewrite (`bee784df`)
        // reworded the emit site and left the filter matching nothing —
        // the metric stayed flat 0 through the 0454 outage. Prose is
        // for operators and may be reworded freely; the `alarm` field
        // is emitted by both hard-failure sites (`handler/mod.rs`
        // post-retry reconcile failure, `main.rs` mTLS cold-start) and
        // exists only for this filter. The declared-vs-emitted infra
        // test fails CI if the pair ever splits (task 0455).
        filterPattern: logs.FilterPattern.stringValue(
          '$.fields.alarm',
          '=',
          'ch_write_failure'
        ),
        metricNamespace: 'SorobanBlockExplorer/Indexer',
        metricName: 'ChWriteFailures',
        metricValue: '1',
        defaultValue: 0,
      }
    );
    withActions(
      new cloudwatch.Alarm(this, 'IndexerChWriteFailureAlarm', {
        alarmName: `${config.envName}-indexer-ch-write-failures`,
        alarmDescription:
          'Indexer Lambda logged a CH write failure (post-retry hard error or mTLS init failure).',
        metric: chWriteFailureFilter.metric({
          period: cdk.Duration.minutes(5),
          statistic: cloudwatch.Stats.SUM,
        }),
        // Zero-tolerance threshold (operator decision 2026-08-18, same
        // rule as the 5xx alarm): ONE failure line = page. A line here is
        // already post-filter — it means a reconcile exhausted the whole
        // in-band retry envelope, not a single flaky request — so it is
        // never routine. An earlier draft used >10 to absorb a planned
        // Caddy reload (worst case ~6 lines), but that is exactly the
        // suppression logic this task rejected for pauses and for 5xx:
        // one knowing page during own maintenance is cheap, and >10
        // would ALSO hide the slow modes forever — a single poison-pill
        // ledger (the 0454 shape) emits only 1-2 lines per window and
        // would never cross 10. Raise the threshold only with a
        // measurement: if routine maintenance pages more than about once
        // a month, record the observed line counts and set it just above
        // them.
        threshold: 0,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): this is a
        // failure COUNTER — months of silence are its healthy steady
        // state, and the filter's defaultValue 0 only appears in periods
        // where the Lambda logged anything at all. Fully-missing data
        // means "no invocations", which is a planned pause or a stall —
        // the backlog-age alarm (Alarm 1a) owns that; BREACHING here
        // would page on every planned pause.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 3: DLQ depth
    //
    // A LEVEL alarm on purpose — the zero-tolerance shape (lore-0455, same
    // philosophy as the 5xx alarm): the DLQ's steady state is EMPTY, so any
    // content is an event. What lands here is only "our side failed" —
    // doorbells that failed reconcile maxReceiveCount times during a CH/S3
    // incident. Re-arm answer (rule 2, ADR 0054): drain per
    // docs/runbooks/dlq.md — doorbells carry no data (the indexer reconciles
    // from the durable cursor), so after the incident PURGE the queue and
    // the alarm returns to OK, re-armed. Standing content is never
    // accepted; the historical 15-day latch was a missing drain procedure,
    // not a detection failure. A DIFF()-growth variant was considered and
    // withdrawn — see ADR 0054.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(this, 'DlqDepthAlarm', {
        alarmName: `${config.envName}-ledger-processor-dlq-depth`,
        alarmDescription:
          'Ledger Processor DLQ has messages - reconcile failed repeatedly during an incident. Runbook: docs/runbooks/dlq.md (inspect, fix cause, then purge - doorbells carry no data).',
        metric: new cloudwatch.Metric({
          namespace: 'AWS/SQS',
          metricName: 'ApproximateNumberOfMessagesVisible',
          dimensionsMap: { QueueName: deadLetterQueue.queueName },
          period: cdk.Duration.minutes(1),
          statistic: cloudwatch.Stats.MAXIMUM,
          label: 'DLQ depth',
        }),
        threshold: 0,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): SQS stops
        // publishing for a queue with ~6 h of no activity, so missing
        // data is the healthy idle-empty state; any depth > 0 resumes
        // publishing and trips the alarm on a single datapoint.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 5b: Type-1 enrichment DLQ depth (task 0191)
    //
    // Same zero-tolerance level shape as Alarm 3. What lands here: DB-write
    // failures during a CH incident (redrive material) and worker
    // crash/timeout poison pills (reproduction evidence). Dead issuer
    // domains — historically 100% of this queue's traffic (measured 30
    // days: 6 keys, ~1000 retries, zero genuine blips) — no longer arrive:
    // connect-level fetch failures classify permanent and sentinel
    // immediately (enrichment-shared http_transient.rs, 2026-08-11).
    // Re-arm answer: fix the cause, then REDRIVE per docs/runbooks/dlq.md.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(this, 'EnrichmentDlqDepthAlarm', {
        alarmName: `${config.envName}-enrichment-dlq-depth`,
        alarmDescription:
          'Enrichment worker DLQ has messages - a DB incident or a poison-pill message (dead-domain fetches sentinel instead of landing here). Runbook: docs/runbooks/dlq.md (inspect, fix cause, then redrive).',
        metric: new cloudwatch.Metric({
          namespace: 'AWS/SQS',
          metricName: 'ApproximateNumberOfMessagesVisible',
          dimensionsMap: { QueueName: enrichmentDlq.queueName },
          period: cdk.Duration.minutes(1),
          statistic: cloudwatch.Stats.MAXIMUM,
          label: 'Enrichment DLQ depth',
        }),
        threshold: 0,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING — same idle-empty rationale as Alarm 3.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 5c: Type-1 enrichment worker error rate (task 0191)
    // Mirrors Alarm 2 (ProcessorErrorRateAlarm) but for the worker
    // Lambda. Uses the same threshold config — both Lambdas have
    // similar acceptable error rates.
    // ---------------------
    const workerErrors = enrichmentWorkerFunction.metricErrors({
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });
    const workerInvocations = enrichmentWorkerFunction.metricInvocations({
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });
    withActions(
      new cloudwatch.Alarm(this, 'EnrichmentWorkerErrorRateAlarm', {
        alarmName: `${config.envName}-enrichment-worker-error-rate`,
        alarmDescription:
          'Enrichment worker Lambda error rate exceeded threshold - DB / network / SEP-1 issues.',
        metric: new cloudwatch.MathExpression({
          expression: 'errors / invocations',
          usingMetrics: {
            errors: workerErrors,
            invocations: workerInvocations,
          },
          period: cdk.Duration.minutes(5),
          label: 'Worker Error Rate',
        }),
        threshold: config.processorErrorRateThreshold,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): no datapoint
        // means 0 invocations, and for this worker that is a NORMAL
        // state — the consumer is deliberately gated off in prod until
        // the 0301 rollout, and even enabled it only runs when the
        // producer publishes misses. BREACHING would page continuously.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 6: any API Gateway 5xx
    //
    // Every 5xx is a defect, not a health indicator to tolerate (lore-0455).
    // Measured before this rewrite: 30 days held 80 gateway 5xx, ALL of them
    // real backend errors in three root-cause classes (CH 60/241/48), every
    // one pre-launch or a since-fixed query — base rate at rewrite time was
    // 0 for 24 straight days. At base 0 a single 5xx IS an event, so the
    // alarm is a bare count: no ratio math (percent-of-a-tiny-denominator
    // was the old alarm's noise source — 28 notifications for those 80
    // errors), no threshold knob. If this alarm starts paging regularly,
    // the fix is to repair the 5xx class it points at — never to widen
    // this alarm. Investigate with Logs Insights on the API log group:
    // filter level="ERROR", group by fields.error / fields.message.
    //
    // Paging shape: CloudWatch notifies on state transition, so a burst is
    // one page (ALARM holds while errors continue) and the alarm re-arms
    // itself after one clean window — rule 2 of ADR 0054.
    //
    // Caveat: gateway 5XXError also counts 502/504 the Lambda log never
    // sees (no access logging on the stage — deliberate, add only when a
    // silent-504 investigation actually needs it).
    // ---------------------
    const stageName = restApi.deploymentStage.stageName;
    const apiName = restApi.restApiName;

    withActions(
      new cloudwatch.Alarm(this, 'ApiGateway5xxAlarm', {
        // Renamed from `-api-gateway-5xx-rate`: the alarm no longer measures
        // a rate, and a name that lies is how the next reader mistrusts the
        // whole set. CloudFormation replaces the alarm on rename — state
        // history restarts, accepted (same call as the DLQ growth renames).
        alarmName: `${config.envName}-api-gateway-5xx`,
        alarmDescription:
          'An API request returned 5xx - a user saw a server error. Every 5xx is a defect: query the API log group in Logs Insights (filter level="ERROR", group by fields.error) and account for each error; do not tune this alarm.',
        metric: new cloudwatch.Metric({
          namespace: 'AWS/ApiGateway',
          metricName: '5XXError',
          dimensionsMap: { ApiName: apiName, Stage: stageName },
          period: cdk.Duration.minutes(5),
          statistic: cloudwatch.Stats.SUM,
        }),
        threshold: 0,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): no datapoint
        // means the stage served zero requests in 5 min, and zero traffic
        // is not a server-error condition. Reachability of the public
        // entry point is the origin-lock canary's job (which pages
        // BREACHING on silence).
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Origin-lockdown canary (task 0277 / ADR 0048, Step 7)
    // ---------------------
    // Periodically asserts the direct-origin bypass vectors stay BLOCKED
    // (403): the raw execute-api URL and the *.cloudfront.net domain. Alarms
    // via the same SNS→Slack topic if either starts answering 2xx — i.e. the
    // Cloudflare-only origin lockdown regressed. Enable only post-cutover
    // (validateConfig warns if the locks are off). ~15-min cadence keeps the
    // canary run cost negligible (~$3-4/mo).
    if (config.enableOriginLockCanary) {
      const canary = new synthetics.Canary(this, 'OriginLockCanary', {
        // AWS Synthetics canary names are capped at 21 chars (and lowercase);
        // `${envName}-origin-lock` = 22 for "production" would pass synth but
        // fail CreateCanary at deploy. Fixed short name (production is the only
        // env; see infrastructure-overview §7.1).
        canaryName: 'soroban-origin-lock',
        runtime: synthetics.Runtime.SYNTHETICS_NODEJS_PUPPETEER_13_0,
        test: synthetics.Test.custom({
          code: synthetics.Code.fromInline(originLockCanaryCode()),
          handler: 'index.handler',
        }),
        schedule: synthetics.Schedule.rate(cdk.Duration.minutes(15)),
        environmentVariables: {
          // Only probe a vector whose lock is actually live. With its lock
          // off an origin legitimately returns 2xx, so probing it would make
          // the canary alarm forever during a staged (one-leg-at-a-time)
          // rollout. The raw execute-api URL returns 403 under EITHER API lock:
          // the app-layer edge-secret check (missing X-Edge-Secret) OR mTLS
          // (disableExecuteApiEndpoint) — so probe it whenever either is live.
          // Gating only on enableApiMtls would leave the edge-secret-only prod
          // config (the current one) with no API target → every run fails.
          ...((config.enableApiMtls || config.enableEdgeSecretLock) && {
            EXECUTE_API_URL: restApi.url,
          }),
          ...(config.enableOriginSecretLock &&
            spaDistributionDomainName && {
              CLOUDFRONT_URL: `https://${spaDistributionDomainName}/`,
            }),
        },
      });

      withActions(
        new cloudwatch.Alarm(this, 'OriginLockCanaryAlarm', {
          alarmName: `${config.envName}-origin-lock-bypass`,
          alarmDescription:
            'Origin-lockdown canary failed - a direct origin (execute-api / *.cloudfront.net) is answering instead of returning 403. Possible Cloudflare-bypass regression.',
          metric: canary.metricSuccessPercent({
            period: cdk.Duration.minutes(15),
          }),
          threshold: 100,
          comparisonOperator: cloudwatch.ComparisonOperator.LESS_THAN_THRESHOLD,
          // Require 2 consecutive 15-min windows (~30 min) below 100% before
          // paging — absorbs the cold-start gap before the first run and a
          // single slow/missed run, while still catching a sustained
          // regression or a stalled canary.
          evaluationPeriods: 2,
          datapointsToAlarm: 2,
          // Security invariant: no fresh confirmation = treat as a breach,
          // so a stalled canary also pages rather than failing silent.
          treatMissingData: cloudwatch.TreatMissingData.BREACHING,
        })
      );
    }

    // ---------------------
    // Dashboard
    //
    // Convention (task 0455, `docs/runbooks/health.md` "four sentences"):
    // the dashboard answers WHERE. It carries what the alarms read — same
    // metric, same queue, same math, so an ALARM state is confirmable on
    // sight — plus a few standing conditions deliberately left unalarmed
    // (durations, concurrency, 4xx). A widget whose signal nothing emits
    // is a defect, not decoration: it implies coverage (see the row-6
    // tombstone below).
    // ---------------------
    new cloudwatch.Dashboard(this, 'Dashboard', {
      dashboardName: `${config.envName}-soroban-explorer`,
      widgets: [
        // Row 1: Ingestion section header
        [
          new cloudwatch.TextWidget({
            markdown: '## Ingestion',
            width: 24,
            height: 1,
          }),
        ],
        // Row 2: Galexie doorbell rate + last indexed ledger + Processor duration.
        // The doorbell widget reads the SAME SQS metric as the
        // `galexie-ingestion-lag` alarm (task 0367 moved the alarm off Lambda
        // invocations; the widget lagged on the old signal until 0455).
        // Invocations lie in both directions: one reconcile drains backlog
        // for up to 9 min (sparse starts look like a dead producer during a
        // healthy catch-up), and a 5-second retry loop looks busy while
        // nothing persists (the 0454 outage). Doorbells count actual S3
        // object landings — ~1 per ledger close. 1-min period is finer than
        // the alarm's 5-min window by intent: same signal, more texture.
        // The backlog-age widget mirrors the `ingestion-backlog-age` alarm
        // (same metric, same 1-min MAXIMUM) with the paging threshold drawn
        // as a horizontal line, and doubles as the reference series the
        // sequence widget lacks: healthy is 0-1 s (732 h: median 0, p90 1);
        // flat sequence + climbing age = the consumer stalled, whatever the
        // cause. Known blind spot, covered by the DLQ pair: when failures
        // drain to the DLQ the main queue empties and age reads green.
        [
          new cloudwatch.GraphWidget({
            title: 'Galexie doorbell rate (ledgers -> ingest queue/min)',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/SQS',
                metricName: 'NumberOfMessagesSent',
                dimensionsMap: { QueueName: ingestQueue.queueName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.SUM,
                label: 'Doorbells',
              }),
            ],
            width: 6,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Ingest backlog age (s, oldest doorbell)',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/SQS',
                metricName: 'ApproximateAgeOfOldestMessage',
                dimensionsMap: { QueueName: ingestQueue.queueName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'Oldest doorbell age',
              }),
            ],
            leftAnnotations: [
              {
                value: config.ingestionBacklogAgeSeconds,
                label: 'pages after 3 consecutive min above',
              },
            ],
            width: 6,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Last processed ledger sequence',
            left: [
              new cloudwatch.Metric({
                namespace: 'SorobanBlockExplorer/Indexer',
                metricName: 'LastProcessedLedgerSequence',
                dimensionsMap: { Environment: config.envName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'Last indexed ledger',
              }),
            ],
            width: 6,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Ledger Processor duration (p50/p95/p99)',
            left: [
              processorFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p50',
                label: 'p50',
              }),
              processorFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p95',
                label: 'p95',
              }),
              processorFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p99',
                label: 'p99',
              }),
            ],
            width: 6,
            height: 6,
          }),
        ],
        // Row 3: Processor errors + DLQ depth
        [
          new cloudwatch.GraphWidget({
            // Two series because the processor fails in two disjoint modes
            // and each is invisible in the other's metric: a crash raises
            // Lambda `Errors` but a failed CH write does NOT (the handler
            // reports batch-item failure so SQS redelivers — Errors stayed 0
            // through the whole 0454 outage), while `ChWriteFailures` (the
            // filter-minted metric the zero-tolerance alarm reads) counts
            // exactly those quiet failures. One glance answers "is the
            // processor failing" in both modes.
            title: 'Ledger Processor errors + CH write failures',
            left: [
              processorFunction.metricErrors({
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'Lambda errors',
              }),
              new cloudwatch.Metric({
                namespace: 'SorobanBlockExplorer/Indexer',
                metricName: 'ChWriteFailures',
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'CH write failures',
              }),
            ],
            width: 6,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Ledger Processor DLQ depth',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/SQS',
                metricName: 'ApproximateNumberOfMessagesVisible',
                dimensionsMap: { QueueName: deadLetterQueue.queueName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'DLQ depth',
              }),
            ],
            width: 6,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Enrichment DLQ depth',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/SQS',
                metricName: 'ApproximateNumberOfMessagesVisible',
                dimensionsMap: { QueueName: enrichmentDlq.queueName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'Enrichment DLQ depth',
              }),
            ],
            width: 6,
            height: 6,
          }),
          // The last alarm that had no widget (2026-08-04 survey). Raw error
          // COUNT, deliberately not the alarm's errors/invocations ratio: the
          // ratio is unreadable at this worker's traffic (a 1-of-1 window is
          // 100%), and the count is what an operator compares against the DLQ
          // depth beside it — errors climbing while the DLQ stays flat means
          // the retries are absorbing them.
          new cloudwatch.GraphWidget({
            title: 'Enrichment worker errors',
            left: [
              enrichmentWorkerFunction.metricErrors({
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'Errors',
              }),
            ],
            width: 6,
            height: 6,
          }),
        ],
        // Row 3b: standing context, deliberately unalarmed (four sentences,
        // rule 3). Kept off the failure row above so that row reads as
        // "every one of these has an alarm behind it".
        [
          new cloudwatch.GraphWidget({
            title: 'Lambda concurrent executions',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/Lambda',
                metricName: 'ConcurrentExecutions',
                dimensionsMap: {
                  FunctionName: processorFunction.functionName,
                },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'Processor',
              }),
              new cloudwatch.Metric({
                namespace: 'AWS/Lambda',
                metricName: 'ConcurrentExecutions',
                dimensionsMap: { FunctionName: apiFunction.functionName },
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'API',
              }),
            ],
            width: 12,
            height: 6,
          }),
        ],
        // Row 4: API section header
        [
          new cloudwatch.TextWidget({
            markdown: '## API',
            width: 24,
            height: 1,
          }),
        ],
        // Row 5: API latency + 4xx/5xx
        [
          new cloudwatch.GraphWidget({
            title: 'API Lambda latency (p50/p95/p99)',
            left: [
              apiFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p50',
                label: 'p50',
              }),
              apiFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p95',
                label: 'p95',
              }),
              apiFunction.metric('Duration', {
                period: cdk.Duration.minutes(5),
                statistic: 'p99',
                label: 'p99',
              }),
            ],
            width: 12,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'API Gateway 4xx / 5xx errors',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/ApiGateway',
                metricName: '4XXError',
                dimensionsMap: { ApiName: apiName, Stage: stageName },
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: '4xx',
              }),
              new cloudwatch.Metric({
                namespace: 'AWS/ApiGateway',
                metricName: '5XXError',
                dimensionsMap: { ApiName: apiName, Stage: stageName },
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: '5xx',
              }),
            ],
            width: 12,
            height: 6,
          }),
        ],
        // Row 6 (cache hit/miss + cold starts) removed in task 0455 — both
        // graphed metrics that production never emits, so both rendered
        // empty from the day they were written:
        // - `CacheHitCount` / `CacheMissCount` need a stage cache cluster.
        //   `apiGatewayCacheEnabled` is false and stays false by decision;
        //   rationale and return condition in
        //   `docs/architecture/backend/api-gateway-cache-spec.md`.
        // - `InitDuration` is NOT a CloudWatch metric at all (verified
        //   2026-08-14: list-metrics empty, zero datapoints over 7 days).
        //   Lambda reports it only on the REPORT log line, so cold starts
        //   are a Logs Insights question (`@initDuration`), not a widget.
        //   Reinstating one means minting the metric from logs first.
        // An empty widget is worse than no widget: it implies coverage.
        //
        // Row 7: Cost — the dashboard answer the cost-anomaly alert lacked
        // (task 0449 acceptance criterion).
        //
        // Cost Anomaly Detection publishes NO CloudWatch metric, so nothing
        // can graph the alert itself; `AWS/Billing EstimatedCharges` is the
        // only graphable spend signal. Three properties to read it correctly,
        // all of them AWS behaviour rather than choices made here:
        //   * it is CUMULATIVE month-to-date, not per-day — the slope is the
        //     daily burn and the line resets to zero on the 1st;
        //   * it refreshes every ~6 h, so it is a trend, never a live number;
        //   * it is published only in us-east-1, hence the explicit region.
        // Account-wide by design (both projects): that is exactly the scope
        // the anomaly monitor watches, so alert and graph agree. Per-project
        // attribution is a Cost Explorer question — docs/runbooks/costs.md.
        //
        // What this catches that the anomaly monitor does not: slow creep.
        // A monitor learns a baseline and fires on step changes; spend that
        // rises a little every day never looks like a step. Budgets were the
        // designed answer and were dropped 2026-08-10, so this graph is the
        // only place a human sees creep at all.
        [
          new cloudwatch.TextWidget({
            markdown: '## Cost',
            width: 24,
            height: 1,
          }),
        ],
        [
          new cloudwatch.GraphWidget({
            title:
              'Account charges, month-to-date (USD, cumulative, both projects)',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/Billing',
                metricName: 'EstimatedCharges',
                dimensionsMap: { Currency: 'USD' },
                region: 'us-east-1',
                period: cdk.Duration.hours(6),
                statistic: cloudwatch.Stats.MAXIMUM,
                label: 'Month-to-date',
              }),
            ],
            width: 12,
            height: 6,
          }),
        ],
        //
        // Resources widgets (RDS CPU / connections / free storage) removed
        // in task 0239 — the production data plane lives on Hetzner
        // ClickHouse, monitored separately on the box.
      ],
    });

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');
  }
}
