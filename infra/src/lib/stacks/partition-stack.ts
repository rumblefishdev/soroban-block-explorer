import * as cdk from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import * as cr from 'aws-cdk-lib/custom-resources';
import * as events from 'aws-cdk-lib/aws-events';
import * as targets from 'aws-cdk-lib/aws-events-targets';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import { RustFunction } from 'cargo-lambda-cdk';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';
import { mtlsSecretArn, secretsManagerLayerArn } from '../mtls.js';

export interface PartitionStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly cargoWorkspacePath: string;
}

/**
 * Partition management stack for the Soroban Block Explorer.
 *
 * Runs a Lambda on every deploy (CDK custom resource) AND on a daily
 * EventBridge schedule. Publishes per-table CloudWatch metrics.
 *
 * Post-task-0239: Lambda runs OUT-of-VPC, reaches Hetzner CH over
 * mTLS using the `lambda-partition-<env>` client cert. The actual
 * partition-management semantics (PG RANGE partitions → CH custom
 * partitioning) are out of scope for 0239; this stack only wires
 * the new transport.
 */
export class PartitionStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: PartitionStackProps) {
    super(scope, id, props);

    const { config, cargoWorkspacePath } = props;

    const metricsNamespace = `SorobanExplorer/${config.envName}/Partitions`;
    const secretName = `${config.mtlsSecretNamePrefix}/lambda-partition-${config.envName}`;

    const secretsExtensionLayer = lambda.LayerVersion.fromLayerVersionArn(
      this,
      'SecretsExtensionLayer',
      secretsManagerLayerArn(this.region)
    );

    // ---------------------
    // Partition Lambda
    // ---------------------
    const partitionFn = new RustFunction(this, 'PartitionFunction', {
      functionName: `${config.envName}-soroban-explorer-partition-mgmt`,
      manifestPath: cargoWorkspacePath,
      binaryName: 'db-partition-mgmt',
      architecture: lambda.Architecture.ARM_64,
      layers: [secretsExtensionLayer],
      memorySize: 256,
      timeout: cdk.Duration.minutes(5),
      logRetention: logs.RetentionDays.ONE_MONTH,
      environment: {
        ENV_NAME: config.envName,
        RUST_LOG: 'info',
        CH_DOMAIN: config.chDomainName,
        MTLS_SECRET_NAME: secretName,
        PARAMETERS_SECRETS_EXTENSION_CACHE_ENABLED: 'true',
      },
    });

    partitionFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['secretsmanager:GetSecretValue'],
        resources: [mtlsSecretArn(this, secretName)],
      })
    );

    partitionFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['cloudwatch:PutMetricData'],
        resources: ['*'],
        conditions: {
          StringEquals: { 'cloudwatch:namespace': metricsNamespace },
        },
      })
    );

    // ---------------------
    // CDK Custom Resource (runs on deploy)
    // ---------------------
    const provider = new cr.Provider(this, 'PartitionProvider', {
      onEventHandler: partitionFn,
      logRetention: logs.RetentionDays.ONE_MONTH,
    });

    new cdk.CustomResource(this, 'EnsurePartitions', {
      serviceToken: provider.serviceToken,
      properties: {
        partitionVersion: Date.now().toString(),
      },
    });

    // ---------------------
    // EventBridge Schedule (daily)
    // ---------------------
    new events.Rule(this, 'DailyPartitionRule', {
      ruleName: `${config.envName}-partition-daily`,
      description:
        'Ensure future partitions + refresh FuturePartitionCount metric daily',
      schedule: events.Schedule.cron({
        minute: '0',
        hour: '2',
      }),
      targets: [new targets.LambdaFunction(partitionFn)],
    });

    // ---------------------
    // CloudWatch Alarms
    // ---------------------
    const timePartitionedTables = [
      'transactions',
      'operations',
      'transaction_participants',
      'soroban_invocations',
      'soroban_events',
      'liquidity_pool_snapshots',
    ];

    for (const table of timePartitionedTables) {
      new cloudwatch.Alarm(this, `FuturePartitions-${table}`, {
        alarmName: `${config.envName}-partition-future-low-${table}`,
        alarmDescription: `Fewer than 2 future partitions for ${table}`,
        metric: new cloudwatch.Metric({
          namespace: metricsNamespace,
          metricName: 'FuturePartitionCount',
          dimensionsMap: { Table: table },
          period: cdk.Duration.days(1),
          statistic: 'Minimum',
        }),
        threshold: 2,
        comparisonOperator: cloudwatch.ComparisonOperator.LESS_THAN_THRESHOLD,
        evaluationPeriods: 1,
        treatMissingData: cloudwatch.TreatMissingData.BREACHING,
      });
    }

    new cloudwatch.Alarm(this, 'PartitionLambdaErrors', {
      alarmName: `${config.envName}-partition-lambda-errors`,
      alarmDescription: 'Partition management Lambda invocation errors',
      metric: partitionFn.metricErrors({
        period: cdk.Duration.days(1),
        statistic: 'Sum',
      }),
      threshold: 0,
      comparisonOperator: cloudwatch.ComparisonOperator.GREATER_THAN_THRESHOLD,
      evaluationPeriods: 1,
      treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
    });

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');
  }
}
