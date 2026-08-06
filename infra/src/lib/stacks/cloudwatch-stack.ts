import * as cdk from 'aws-cdk-lib';
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
          'No new ledgers landed in S3 (0 doorbells to the ingest queue) for the lag window — Galexie may have stopped writing.',
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
    // Alarm 1b: Galexie ephemeral storage utilization (%)
    // captive-core's BucketList (current ledger state) + catchup temp live on
    // the task's ephemeral disk. Baseline ~30%; >60% sustained = plan a disk
    // bump BEFORE a merge/catchup spike hits the "No space left on device"
    // ceiling (the 2026-07-01/02 deadlock: full disk → catchup never completes
    // → temp never cleaned → task wedged while `pgrep stellar-core` still
    // reports healthy). Metric-math on % is robust to disk-size changes.
    // Sustained 3×5 min avoids paging on a transient merge spike.
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
          'Galexie captive-core ephemeral disk >60% — approaching the deadlock ceiling; plan a disk bump.',
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
          'Ledger Processor error rate exceeded threshold — ledgers may be failing to index.',
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
        // 30-day window (0454). Absence coverage is the stall-alarm pair's
        // job, not this alarm's.
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
        // Threshold in DOORBELL-INVOCATION units (one failure line per
        // failed reconcile, task 0241 — not per ledger): a sustained CH
        // outage fails ~50-60 fresh doorbells per 5-min window (one per
        // ledger close every ~5-6 s), far above 10, so it fires in the
        // first window. A planned ~30 s Caddy reload is absorbed by the
        // in-band retry envelope; worst case ≤6 failure lines < 10 = no
        // page. Raise only on observed false-alarms during routine
        // maintenance.
        threshold: 10,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): this is a
        // failure COUNTER — months of silence are its healthy steady
        // state, and the filter's defaultValue 0 only appears in periods
        // where the Lambda logged anything at all. Fully-missing data
        // means "no invocations", which is a planned pause or a stall —
        // the stall-alarm pair owns that; BREACHING here would page on
        // every planned pause.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 3: DLQ depth
    // Any message landing in the DLQ means a ledger permanently failed processing.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(this, 'DlqDepthAlarm', {
        alarmName: `${config.envName}-ledger-processor-dlq-depth`,
        alarmDescription:
          'Ledger Processor DLQ has messages — one or more ledgers permanently failed processing.',
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
        // data is the healthy idle-empty state; any depth > 0 publishes.
        // KNOWN DEFECT (0455 defect 4, fix pending): this is a LEVEL
        // alarm — once non-empty it latches in ALARM and goes mute for
        // every later incident (observed: 15 days latched while a lag
        // event passed unseen). Conversion to a growth-based alarm is
        // the umbrella's re-arm work.
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      })
    );

    // ---------------------
    // Alarm 5b: Type-1 enrichment DLQ depth (task 0191)
    // Any message landing in the enrichment DLQ means an asset
    // permanently failed enrichment after maxReceiveCount=3 retries.
    // Same shape as Alarm 5.
    // ---------------------
    withActions(
      new cloudwatch.Alarm(this, 'EnrichmentDlqDepthAlarm', {
        alarmName: `${config.envName}-enrichment-dlq-depth`,
        alarmDescription:
          'Enrichment worker DLQ has messages — one or more assets permanently failed enrichment.',
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
        // NOT_BREACHING + latching level-alarm: same rationale and same
        // known defect as the ledger DLQ alarm above (observed latched 32
        // days while its queue grew). Growth-based conversion pending.
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
          'Enrichment worker Lambda error rate exceeded threshold — DB / network / SEP-1 issues.',
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
    // Alarm 6: API Gateway 5xx rate
    // 5xxError / Count > threshold over 5-minute window.
    // ---------------------
    const stageName = restApi.deploymentStage.stageName;
    const apiName = restApi.restApiName;

    const api5xx = new cloudwatch.Metric({
      namespace: 'AWS/ApiGateway',
      metricName: '5XXError',
      dimensionsMap: { ApiName: apiName, Stage: stageName },
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });
    const apiCount = new cloudwatch.Metric({
      namespace: 'AWS/ApiGateway',
      metricName: 'Count',
      dimensionsMap: { ApiName: apiName, Stage: stageName },
      period: cdk.Duration.minutes(5),
      statistic: cloudwatch.Stats.SUM,
    });

    withActions(
      new cloudwatch.Alarm(this, 'ApiGateway5xxAlarm', {
        alarmName: `${config.envName}-api-gateway-5xx-rate`,
        alarmDescription:
          'API Gateway 5xx error rate exceeded threshold — user-facing errors.',
        metric: new cloudwatch.MathExpression({
          expression: '(m5xx / mcount) * 100',
          usingMetrics: { m5xx: api5xx, mcount: apiCount },
          period: cdk.Duration.minutes(5),
          label: '5xx Rate (%)',
        }),
        threshold: config.apiGateway5xxThreshold,
        comparisonOperator:
          cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
        evaluationPeriods: 1,
        // NOT_BREACHING is correct here (task 0455 review): the ratio has
        // no datapoint only when the stage served zero requests in 5 min,
        // and zero traffic is not a server-error condition. Reachability
        // of the public entry point is the origin-lock canary's job
        // (which pages BREACHING on silence).
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
            'Origin-lockdown canary failed — a direct origin (execute-api / *.cloudfront.net) is answering instead of returning 403. Possible Cloudflare-bypass regression.',
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
        // Row 2: Galexie freshness proxy + last indexed ledger + Processor duration
        [
          new cloudwatch.GraphWidget({
            title: 'Galexie S3 freshness (Processor invocations/min)',
            left: [
              processorFunction.metricInvocations({
                period: cdk.Duration.minutes(1),
                statistic: cloudwatch.Stats.SUM,
                label: 'Invocations',
              }),
            ],
            width: 8,
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
            width: 8,
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
            width: 8,
            height: 6,
          }),
        ],
        // Row 3: Processor errors + DLQ depth
        [
          new cloudwatch.GraphWidget({
            title: 'Ledger Processor errors',
            left: [
              processorFunction.metricErrors({
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'Errors',
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
            width: 6,
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
        // Row 6: API Gateway cache hit rate
        [
          new cloudwatch.GraphWidget({
            title: 'API Gateway cache hit / miss',
            left: [
              new cloudwatch.Metric({
                namespace: 'AWS/ApiGateway',
                metricName: 'CacheHitCount',
                dimensionsMap: { ApiName: apiName, Stage: stageName },
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'Cache hits',
              }),
              new cloudwatch.Metric({
                namespace: 'AWS/ApiGateway',
                metricName: 'CacheMissCount',
                dimensionsMap: { ApiName: apiName, Stage: stageName },
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SUM,
                label: 'Cache misses',
              }),
            ],
            width: 12,
            height: 6,
          }),
          new cloudwatch.GraphWidget({
            title: 'Lambda cold starts',
            left: [
              processorFunction.metric('InitDuration', {
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SAMPLE_COUNT,
                label: 'Processor cold starts',
              }),
              apiFunction.metric('InitDuration', {
                period: cdk.Duration.minutes(5),
                statistic: cloudwatch.Stats.SAMPLE_COUNT,
                label: 'API cold starts',
              }),
            ],
            width: 12,
            height: 6,
          }),
        ],
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
