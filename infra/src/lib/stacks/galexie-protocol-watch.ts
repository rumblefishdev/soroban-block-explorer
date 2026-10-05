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

const HORIZON_BY_PASSPHRASE: Record<string, string> = {
  'Public Global Stellar Network ; September 2015':
    'https://horizon.stellar.org/',
  'Test SDF Network ; September 2015': 'https://horizon-testnet.stellar.org/',
};

export interface GalexieProtocolWatchProps {
  readonly config: EnvironmentConfig;
  readonly galexieCluster: ecs.ICluster;
  readonly galexieService: ecs.IBaseService;
  /** Attaches the stack's alarm and OK actions (Slack). */
  readonly withActions: (alarm: cloudwatch.Alarm) => cloudwatch.Alarm;
}

/**
 * Alarm 1d: our Galexie cannot apply the protocol the network is about to
 * vote, or already runs (task 0610). Every pubnet vote stops a Galexie whose
 * captive core does not support the new protocol — P29 stopped ours for ~14 h
 * on 2026-10-01, though Horizon had reported the new core for nine days.
 *
 * A Lambda reads, every 30 minutes, the captive-core version of the image the
 * Galexie service runs (from ECR, not from tags) and Horizon's protocol
 * numbers; anything but OK, or a read that fails, throws. The alarm watches
 * the function's built-in Errors metric, so there is no custom metric to pay
 * for. The reason is in the function's log.
 */
export function addGalexieProtocolWatch(
  scope: Construct,
  {
    config,
    galexieCluster,
    galexieService,
    withActions,
  }: GalexieProtocolWatchProps
): void {
  const horizonUrl = HORIZON_BY_PASSPHRASE[config.stellarNetworkPassphrase];
  if (!horizonUrl) {
    throw new Error(
      `no Horizon known for passphrase "${config.stellarNetworkPassphrase}"`
    );
  }

  const functionName = `${config.envName}-galexie-protocol-watch`;
  const fn = new lambda.Function(scope, 'GalexieProtocolWatchFunction', {
    functionName,
    description: 'Is our Galexie core ready for the next protocol? (task 0610)',
    runtime: lambda.Runtime.NODEJS_22_X,
    handler: 'index.handler',
    code: lambda.Code.fromAsset(HANDLER_DIR, { exclude: ['__tests__'] }),
    timeout: cdk.Duration.minutes(1),
    memorySize: 128,
    // One run is one answer; the next run asks again. Retries would only
    // multiply a single failure in the Errors metric.
    retryAttempts: 0,
    environment: {
      HORIZON_URL: horizonUrl,
      CLUSTER_NAME: galexieCluster.clusterName,
      SERVICE_NAME: galexieService.serviceName,
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

  new scheduler.Schedule(scope, 'GalexieProtocolWatchSchedule', {
    scheduleName: functionName,
    description: 'Runs the Galexie protocol watch every 30 minutes (task 0610)',
    // Twice per alarm period, so a run landing a few seconds either side of
    // an hour boundary never leaves an hour without a datapoint.
    schedule: scheduler.ScheduleExpression.rate(cdk.Duration.minutes(30)),
    target: new targets.LambdaInvoke(fn, { retryAttempts: 0 }),
  });

  // Two failing hours in a row, so one bad minute at Horizon pages nobody;
  // the vote is days away when this first turns red. BREACHING: a function
  // that stopped running publishes no datapoint, and a watch that checks
  // nothing must not look green.
  withActions(
    new cloudwatch.Alarm(scope, 'GalexieProtocolWatchAlarm', {
      alarmName: `${config.envName}-galexie-protocol-watch`,
      alarmDescription:
        'Our Galexie captive core cannot apply the protocol the network core already supports (bump before the vote) or already runs, or the watch could not check. Reason in the log of the galexie-protocol-watch function. Runbook: docs/runbooks/galexie-protocol-watch.md.',
      metric: fn.metricErrors({
        period: cdk.Duration.hours(1),
        statistic: cloudwatch.Stats.SUM,
      }),
      threshold: 1,
      comparisonOperator:
        cloudwatch.ComparisonOperator.GREATER_THAN_OR_EQUAL_TO_THRESHOLD,
      evaluationPeriods: 2,
      datapointsToAlarm: 2,
      treatMissingData: cloudwatch.TreatMissingData.BREACHING,
    })
  );
}
