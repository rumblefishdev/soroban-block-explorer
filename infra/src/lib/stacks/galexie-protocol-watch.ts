import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as cdk from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import type * as ecs from 'aws-cdk-lib/aws-ecs';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as scheduler from 'aws-cdk-lib/aws-scheduler';
import * as targets from 'aws-cdk-lib/aws-scheduler-targets';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

// infra/lambdas/…, from both src/lib/stacks (tests) and dist/lib/stacks (cdk).
const HANDLER_DIR = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../../../lambdas/galexie-protocol-watch'
);

export interface GalexieProtocolWatchProps {
  readonly config: EnvironmentConfig;
  readonly galexieCluster: ecs.ICluster;
  readonly galexieService: ecs.IBaseService;
  /** The indexer, whose `STELLAR_XDR_MAJOR` the stellar-xdr check reads. */
  readonly indexerFunction: lambda.IFunction;
  /** Attaches the stack's alarm and OK actions (Slack). */
  readonly withActions: (alarm: cloudwatch.Alarm) => cloudwatch.Alarm;
}

/**
 * Alarm 1d: a protocol upgrade is coming and we are not ready for it — a
 * Galexie with a newer core is out and ours is not on it (task 0610), or core
 * supports a protocol, a soroban-env-host on its stellar-xdr is published,
 * and our indexer is not built with it (task 0646). Every pubnet vote stops
 * a Galexie whose captive core does not support the new protocol — P29
 * stopped ours for ~14 h on 2026-10-01, though the image that fixed it had
 * been on Docker Hub for a week.
 *
 * A Lambda reads, every 3 hours, the captive-core version of the image the
 * Galexie service runs (from ECR, not from tags) and of the newest image on
 * Docker Hub, and the indexer's `STELLAR_XDR_MAJOR` against Horizon and
 * crates.io; something to deploy, or a read that fails, throws. The alarm
 * watches the function's built-in Errors metric, so there is no custom metric
 * to pay for. The reason is in the function's log.
 */
export function addGalexieProtocolWatch(
  scope: Construct,
  {
    config,
    galexieCluster,
    galexieService,
    indexerFunction,
    withActions,
  }: GalexieProtocolWatchProps
): void {
  const functionName = `${config.envName}-galexie-protocol-watch`;
  const fn = new lambda.Function(scope, 'GalexieProtocolWatchFunction', {
    functionName,
    description:
      'Is a Galexie with a newer core out, or a stellar-xdr our indexer lacks? (tasks 0610, 0646)',
    runtime: lambda.Runtime.NODEJS_22_X,
    handler: 'index.handler',
    code: lambda.Code.fromAsset(HANDLER_DIR, { exclude: ['__tests__'] }),
    // Every HTTP read gives up after 10 s. A run makes seven in a row (the ECR
    // blob, five to Docker Hub, Horizon), and when our stellar-xdr lags one
    // more to crates.io plus one per soroban-env-common release it reads —
    // usually two, the walk stops at the first not newer than ours. A timeout
    // here would page with no reason in the log.
    timeout: cdk.Duration.minutes(3),
    memorySize: 128,
    // The next scheduled run, 3 hours later, is the retry.
    retryAttempts: 0,
    environment: {
      CLUSTER_NAME: galexieCluster.clusterName,
      SERVICE_NAME: galexieService.serviceName,
      INDEXER_FUNCTION_NAME: indexerFunction.functionName,
    },
    logGroup: new logs.LogGroup(scope, 'GalexieProtocolWatchLogGroup', {
      logGroupName: `/aws/lambda/${functionName}`,
      retention: logs.RetentionDays.ONE_MONTH,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    }),
  });

  // Read-only: the service's task definition, and the image it names.
  // DescribeTaskDefinition takes no resource restriction.
  fn.addToRolePolicy(
    new iam.PolicyStatement({
      actions: ['ecs:DescribeServices'],
      resources: [galexieService.serviceArn],
    })
  );
  fn.addToRolePolicy(
    new iam.PolicyStatement({
      actions: ['ecs:DescribeTaskDefinition'],
      resources: ['*'],
    })
  );
  fn.addToRolePolicy(
    new iam.PolicyStatement({
      actions: ['ecr:BatchGetImage', 'ecr:GetDownloadUrlForLayer'],
      resources: [
        cdk.Stack.of(scope).formatArn({
          service: 'ecr',
          resource: 'repository',
          resourceName: `${config.envName}-galexie`,
        }),
      ],
    })
  );

  // Read-only: the indexer's configuration, for its STELLAR_XDR_MAJOR.
  fn.addToRolePolicy(
    new iam.PolicyStatement({
      actions: ['lambda:GetFunctionConfiguration'],
      resources: [
        cdk.Stack.of(scope).formatArn({
          service: 'lambda',
          resource: 'function',
          // From the name, which the stack already imports: the ARN would
          // add a cross-stack export.
          resourceName: indexerFunction.functionName,
          arnFormat: cdk.ArnFormat.COLON_RESOURCE_NAME,
        }),
      ],
    })
  );

  new scheduler.Schedule(scope, 'GalexieProtocolWatchSchedule', {
    scheduleName: functionName,
    description:
      'Runs the Galexie and stellar-xdr protocol watch every 3 hours (tasks 0610, 0646)',
    // Twice per alarm period, so a run landing a few seconds late never
    // leaves a period without a datapoint.
    schedule: scheduler.ScheduleExpression.rate(cdk.Duration.hours(3)),
    target: new targets.LambdaInvoke(fn, { retryAttempts: 0 }),
  });

  // Both runs of a 6-hour period failed, so one bad minute at Docker Hub
  // pages nobody;
  // the vote is a week or more away when this first turns red. BREACHING: a
  // function that stopped running publishes no datapoint (a run that
  // succeeds publishes Errors = 0), and a watch that checks nothing must not
  // look green.
  withActions(
    new cloudwatch.Alarm(scope, 'GalexieProtocolWatchAlarm', {
      alarmName: `${config.envName}-galexie-protocol-watch`,
      alarmDescription:
        'A protocol upgrade is coming and we are not ready: a Galexie with a newer captive core is on Docker Hub and ours is not on it, or a soroban-env-host on the stellar-xdr of the protocol core supports is published and our indexer is not built with it. Or the watch could not check. Reason in the log of the galexie-protocol-watch function. Runbook: docs/runbooks/galexie-protocol-watch.md.',
      metric: fn.metricErrors({
        period: cdk.Duration.hours(6),
        statistic: cloudwatch.Stats.SUM,
      }),
      threshold: 2,
      comparisonOperator:
        cloudwatch.ComparisonOperator.GREATER_THAN_OR_EQUAL_TO_THRESHOLD,
      evaluationPeriods: 1,
      treatMissingData: cloudwatch.TreatMissingData.BREACHING,
    })
  );
}
