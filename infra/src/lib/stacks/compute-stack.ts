import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as lambdaDestinations from 'aws-cdk-lib/aws-lambda-destinations';
import * as lambdaEventSources from 'aws-cdk-lib/aws-lambda-event-sources';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as s3n from 'aws-cdk-lib/aws-s3-notifications';
import * as sqs from 'aws-cdk-lib/aws-sqs';
import { RustFunction } from 'cargo-lambda-cdk';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';
import { mtlsSecretArn, secretsManagerLayerArn } from '../mtls.js';

const DLQ_RETENTION_DAYS = 14;

export interface ComputeStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly ledgerBucketArn: string;
  readonly ledgerBucketName: string;
  readonly cargoWorkspacePath: string;
}

/**
 * Compute layer for the Soroban Block Explorer.
 *
 * Three Rust Lambda functions built via cargo-lambda-cdk:
 * - API Lambda (axum): serves REST API
 * - Ledger Processor Lambda (indexer): processes S3 PutObject events
 * - Type-1 Enrichment worker Lambda: SEP-1 toml fetch
 *
 * Post-task-0239: all Lambdas run OUTSIDE the VPC (no `vpc`/
 * `securityGroups`/`vpcSubnets`). Egress goes via the AWS-managed
 * Lambda pool. Identity to the Hetzner-hosted ClickHouse box is
 * proven by mTLS client certs sourced from Secrets Manager via the
 * AWS Parameters and Secrets Lambda Extension layer; the Lambda
 * code reads the per-service bundle (`{cert, key, ca}` per task 0240)
 * from the local extension HTTP cache at cold start.
 */
export class ComputeStack extends cdk.Stack {
  readonly apiFunction: lambda.IFunction;
  readonly processorFunction: lambda.IFunction;
  readonly deadLetterQueue: sqs.IQueue;
  readonly enrichmentDlq: sqs.IQueue;
  readonly enrichmentWorkerFunction: lambda.IFunction;

  constructor(scope: Construct, id: string, props: ComputeStackProps) {
    super(scope, id, props);

    const { config, ledgerBucketArn, ledgerBucketName, cargoWorkspacePath } =
      props;

    const ledgerBucket = s3.Bucket.fromBucketAttributes(this, 'LedgerBucket', {
      bucketArn: ledgerBucketArn,
      bucketName: ledgerBucketName,
    });

    const apiLogGroup = new logs.LogGroup(this, 'ApiLogGroup', {
      logGroupName: `/aws/lambda/${config.envName}-soroban-explorer-api`,
      retention: logs.RetentionDays.ONE_MONTH,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    const processorLogGroup = new logs.LogGroup(this, 'ProcessorLogGroup', {
      logGroupName: `/aws/lambda/${config.envName}-soroban-explorer-indexer`,
      retention: logs.RetentionDays.ONE_MONTH,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    const enrichmentWorkerLogGroup = new logs.LogGroup(
      this,
      'EnrichmentWorkerLogGroup',
      {
        logGroupName: `/aws/lambda/${config.envName}-soroban-explorer-enrichment-worker`,
        retention: logs.RetentionDays.ONE_MONTH,
        removalPolicy: cdk.RemovalPolicy.DESTROY,
      }
    );

    // ---------------------
    // Shared mTLS extension layer
    // ---------------------
    // AWS-managed "Parameters and Secrets Lambda Extension" — caches
    // Secrets Manager values in-memory and exposes them via a
    // localhost HTTP API (port 2773). Lambda code reads the
    // `{cert, key, ca}` bundle at cold start and configures its CH
    // client; per-invocation reads hit the in-process cache, so
    // there's no SM API call on the hot path.
    const secretsExtensionLayer = lambda.LayerVersion.fromLayerVersionArn(
      this,
      'SecretsExtensionLayer',
      secretsManagerLayerArn(this.region)
    );

    const sharedLambdaProps = {
      architecture: lambda.Architecture.ARM_64,
      tracing: lambda.Tracing.ACTIVE,
      layers: [secretsExtensionLayer],
    };

    const sharedEnv = {
      ENV_NAME: config.envName,
      // Task 0160 — indexer derives SAC `contract_id` deterministically
      // (`SHA256(network_id || XDR(ContractIdPreimage))`) and panics if
      // this passphrase is missing. Same value used by Galexie partition
      // mapping in ingestion-stack — single source of truth.
      STELLAR_NETWORK_PASSPHRASE: config.stellarNetworkPassphrase,
      // mTLS endpoint on the Hetzner box (provisioned by HetznerDnsStack).
      CH_DOMAIN: config.chDomainName,
      // Standard config for the Secrets Manager Lambda Extension —
      // turn on in-memory caching so repeat reads in the same execution
      // environment hit RAM, not Secrets Manager.
      PARAMETERS_SECRETS_EXTENSION_CACHE_ENABLED: 'true',
    };

    // ---------------------
    // SQS Dead-Letter Queue
    // ---------------------
    const dlq = new sqs.Queue(this, 'ProcessorDlq', {
      queueName: `${config.envName}-ledger-processor-dlq`,
      retentionPeriod: cdk.Duration.days(DLQ_RETENTION_DAYS),
    });
    this.deadLetterQueue = dlq;

    // ---------------------
    // Type-1 Enrichment Queue (task 0191)
    // ---------------------
    const enrichmentDlq = new sqs.Queue(this, 'EnrichmentDlq', {
      queueName: `${config.envName}-enrichment-dlq`,
      retentionPeriod: cdk.Duration.days(DLQ_RETENTION_DAYS),
    });

    const enrichmentVisibilityTimeoutSeconds = Math.max(
      60,
      config.enrichmentWorkerLambdaTimeout * 6
    );
    const enrichmentQueue = new sqs.Queue(this, 'EnrichmentQueue', {
      queueName: `${config.envName}-enrichment`,
      retentionPeriod: cdk.Duration.days(DLQ_RETENTION_DAYS),
      visibilityTimeout: cdk.Duration.seconds(
        enrichmentVisibilityTimeoutSeconds
      ),
      deadLetterQueue: {
        queue: enrichmentDlq,
        maxReceiveCount: 3,
      },
    });
    this.enrichmentDlq = enrichmentDlq;

    // ---------------------
    // API Lambda
    // ---------------------
    const apiSecretName = `${config.mtlsSecretNamePrefix}/lambda-api-${config.envName}`;
    const apiFunction = new RustFunction(this, 'ApiFunction', {
      functionName: `${config.envName}-soroban-explorer-api`,
      manifestPath: cargoWorkspacePath,
      binaryName: 'api',
      ...sharedLambdaProps,
      logGroup: apiLogGroup,
      memorySize: config.apiLambdaMemory,
      timeout: cdk.Duration.seconds(config.apiLambdaTimeout),
      environment: {
        ...sharedEnv,
        AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
        API_BASE_URL: `https://${config.apiDomainName}`,
        MTLS_SECRET_NAME: apiSecretName,
      },
    });
    this.apiFunction = apiFunction;
    grantMtlsSecretRead(this, apiFunction, apiSecretName);

    // ---------------------
    // Ledger Processor Lambda
    // ---------------------
    const processorSecretName = `${config.mtlsSecretNamePrefix}/lambda-ingestion-${config.envName}`;
    const processorFunction = new RustFunction(this, 'ProcessorFunction', {
      functionName: `${config.envName}-soroban-explorer-indexer`,
      manifestPath: cargoWorkspacePath,
      binaryName: 'indexer',
      ...sharedLambdaProps,
      logGroup: processorLogGroup,
      memorySize: config.indexerLambdaMemory,
      timeout: cdk.Duration.seconds(config.indexerLambdaTimeout),
      reservedConcurrentExecutions: config.indexerLambdaConcurrency,
      environment: {
        ...sharedEnv,
        BUCKET_NAME: ledgerBucket.bucketName,
        RUST_LOG: 'info',
        ENRICHMENT_QUEUE_URL: enrichmentQueue.queueUrl,
        MTLS_SECRET_NAME: processorSecretName,
      },
    });
    this.processorFunction = processorFunction;
    grantMtlsSecretRead(this, processorFunction, processorSecretName);

    new lambda.EventInvokeConfig(this, 'ProcessorInvokeConfig', {
      function: processorFunction,
      retryAttempts: config.indexerLambdaRetryAttempts,
      onFailure: new lambdaDestinations.SqsDestination(dlq),
    });

    if (config.indexerLambdaConcurrency > 0) {
      ledgerBucket.addEventNotification(
        s3.EventType.OBJECT_CREATED,
        new s3n.LambdaDestination(processorFunction),
        { suffix: '.xdr.zst' }
      );
    }

    // ---------------------
    // Type-1 Enrichment Worker Lambda (task 0191)
    // ---------------------
    const enrichmentSecretName = `${config.mtlsSecretNamePrefix}/lambda-enrichment-${config.envName}`;
    const enrichmentWorkerFunction = new RustFunction(
      this,
      'EnrichmentWorkerFunction',
      {
        functionName: `${config.envName}-soroban-explorer-enrichment-worker`,
        manifestPath: cargoWorkspacePath,
        binaryName: 'enrichment-worker',
        ...sharedLambdaProps,
        logGroup: enrichmentWorkerLogGroup,
        memorySize: config.enrichmentWorkerLambdaMemory,
        timeout: cdk.Duration.seconds(config.enrichmentWorkerLambdaTimeout),
        reservedConcurrentExecutions: config.enrichmentWorkerLambdaConcurrency,
        environment: {
          ...sharedEnv,
          RUST_LOG: 'info',
          MTLS_SECRET_NAME: enrichmentSecretName,
        },
      }
    );
    grantMtlsSecretRead(this, enrichmentWorkerFunction, enrichmentSecretName);

    if (config.enrichmentWorkerLambdaConcurrency > 0) {
      enrichmentWorkerFunction.addEventSource(
        new lambdaEventSources.SqsEventSource(enrichmentQueue, {
          batchSize: 10,
          maxBatchingWindow: cdk.Duration.seconds(5),
          reportBatchItemFailures: true,
        })
      );
    }
    this.enrichmentWorkerFunction = enrichmentWorkerFunction;

    // ---------------------
    // IAM Grants (non-mTLS)
    // ---------------------
    ledgerBucket.grantRead(processorFunction);
    processorFunction.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['cloudwatch:PutMetricData'],
        resources: ['*'],
        conditions: {
          StringEquals: {
            'cloudwatch:namespace': 'SorobanBlockExplorer/Indexer',
          },
        },
      })
    );

    enrichmentQueue.grantSendMessages(processorFunction);
    enrichmentQueue.grantConsumeMessages(enrichmentWorkerFunction);

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // ---------------------
    // Outputs
    // ---------------------
    new cdk.CfnOutput(this, 'ApiLambdaArn', {
      value: apiFunction.functionArn,
    });
    new cdk.CfnOutput(this, 'ProcessorLambdaArn', {
      value: processorFunction.functionArn,
    });
    new cdk.CfnOutput(this, 'DlqUrl', {
      value: dlq.queueUrl,
    });
  }
}

/**
 * Grant a Lambda permission to read a specific mTLS secret by name.
 * Uses a wildcard suffix to match the random 6-char tail that AWS
 * appends to every Secrets Manager ARN (e.g. `…/secret:soroban/…-aBcDeF`).
 */
function grantMtlsSecretRead(
  scope: cdk.Stack,
  fn: lambda.IFunction,
  secretName: string
): void {
  fn.addToRolePolicy(
    new iam.PolicyStatement({
      actions: ['secretsmanager:GetSecretValue'],
      resources: [mtlsSecretArn(scope, secretName)],
    })
  );
}
