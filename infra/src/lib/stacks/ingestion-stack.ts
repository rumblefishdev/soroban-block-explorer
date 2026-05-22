import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

const STOP_TIMEOUT_MIN = 2;
const STOP_TIMEOUT_MAX = 120;

function validateStopTimeout(seconds: number): void {
  if (seconds < STOP_TIMEOUT_MIN || seconds > STOP_TIMEOUT_MAX) {
    throw new Error(
      `galexieStopTimeout must be between ${STOP_TIMEOUT_MIN} and ${STOP_TIMEOUT_MAX} seconds, got ${seconds}`
    );
  }
}

function toRetentionDays(days: number): logs.RetentionDays {
  const mapping: Record<number, logs.RetentionDays> = {
    1: logs.RetentionDays.ONE_DAY,
    3: logs.RetentionDays.THREE_DAYS,
    5: logs.RetentionDays.FIVE_DAYS,
    7: logs.RetentionDays.ONE_WEEK,
    14: logs.RetentionDays.TWO_WEEKS,
    30: logs.RetentionDays.ONE_MONTH,
    60: logs.RetentionDays.TWO_MONTHS,
    90: logs.RetentionDays.THREE_MONTHS,
    120: logs.RetentionDays.FOUR_MONTHS,
    150: logs.RetentionDays.FIVE_MONTHS,
    180: logs.RetentionDays.SIX_MONTHS,
    365: logs.RetentionDays.ONE_YEAR,
  };
  const result = mapping[days];
  if (result === undefined) {
    throw new Error(
      `Unsupported ecsLogRetentionDays value: ${days}. ` +
        `Supported: ${Object.keys(mapping).join(', ')}`
    );
  }
  return result;
}

export interface IngestionStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly vpc: ec2.IVpc;
  readonly ecsSecurityGroup: ec2.ISecurityGroup;
  readonly ledgerBucketArn: string;
  readonly ledgerBucketName: string;
}

/**
 * ECS Fargate ingestion layer for the Soroban Block Explorer.
 *
 * Post-task-0239: Galexie runs in the public subnet with
 * `assignPublicIp: ENABLED`. Without NAT GW in the new topology
 * the public IP per task is the ONLY egress path; flipping it
 * back to false breaks every outbound connection (ECR pull, S3,
 * Stellar overlay, Hetzner CH).
 *
 * Cert bundle for the future direct-to-CH write path is injected
 * via ECS native Secrets Manager integration (one env var per
 * `{cert,key,ca}` field), so the container can materialise the
 * PEM files at startup without an SDK call.
 */
export class IngestionStack extends cdk.Stack {
  readonly cluster: ecs.ICluster;
  readonly repository: ecr.IRepository;
  readonly liveService: ecs.FargateService;
  readonly backfillTaskDefinition?: ecs.FargateTaskDefinition;

  constructor(scope: Construct, id: string, props: IngestionStackProps) {
    super(scope, id, props);

    const { config, vpc, ecsSecurityGroup, ledgerBucketArn, ledgerBucketName } =
      props;

    const logRetention = toRetentionDays(config.ecsLogRetentionDays);
    validateStopTimeout(config.galexieStopTimeout);

    const galexieImageTag =
      (this.node.tryGetContext('galexieImageTag') as string) ||
      config.galexieImageTag;

    const ledgerBucket = s3.Bucket.fromBucketAttributes(this, 'LedgerBucket', {
      bucketArn: ledgerBucketArn,
      bucketName: ledgerBucketName,
    });

    // ---------------------
    // ECR Repository
    // ---------------------
    const repository = new ecr.Repository(this, 'GalexieRepo', {
      repositoryName: `${config.envName}-galexie`,
      imageScanOnPush: true,
      encryption: config.kmsEncryption
        ? ecr.RepositoryEncryption.KMS
        : ecr.RepositoryEncryption.AES_256,
      removalPolicy: config.kmsEncryption
        ? cdk.RemovalPolicy.RETAIN
        : cdk.RemovalPolicy.DESTROY,
      emptyOnDelete: !config.kmsEncryption,
      lifecycleRules: [
        {
          description: 'Expire untagged images after 7 days',
          maxImageAge: cdk.Duration.days(7),
          rulePriority: 1,
          tagStatus: ecr.TagStatus.UNTAGGED,
        },
        {
          description: 'Retain last 10 images',
          maxImageCount: 10,
          rulePriority: 2,
          tagStatus: ecr.TagStatus.ANY,
        },
      ],
    });
    this.repository = repository;

    new ssm.StringParameter(this, 'EcrRepoUriParam', {
      parameterName: `/soroban-explorer/${config.envName}/ecr-galexie-repo-uri`,
      stringValue: repository.repositoryUri,
    });

    // ---------------------
    // mTLS client cert bundle for Galexie
    // ---------------------
    // Per-service Secrets Manager bundle (`{cert,key,ca}`); the Caddy
    // CN→user map on the Hetzner box authenticates `galexie-<env>`
    // to the `galexie` ClickHouse user without a password (task 0240
    // proxy-trust model).
    const galexieSecretName = `${config.mtlsSecretNamePrefix}/galexie-${config.envName}`;
    const galexieMtlsSecret = secretsmanager.Secret.fromSecretNameV2(
      this,
      'GalexieMtlsSecret',
      galexieSecretName
    );

    // ---------------------
    // ECS Cluster
    // ---------------------
    const cluster = new ecs.Cluster(this, 'IngestionCluster', {
      clusterName: `${config.envName}-ingestion`,
      vpc,
      containerInsightsV2: ecs.ContainerInsights.ENABLED,
    });
    this.cluster = cluster;

    // ---------------------
    // CloudWatch Log Groups
    // ---------------------
    const liveLogGroup = new logs.LogGroup(this, 'LiveLogGroup', {
      logGroupName: `/ecs/${config.envName}/galexie-live`,
      retention: logRetention,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    const backfillLogGroup = new logs.LogGroup(this, 'BackfillLogGroup', {
      logGroupName: `/ecs/${config.envName}/galexie-backfill`,
      retention: logRetention,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    // ---------------------
    // Galexie config
    // ---------------------
    const galexieNetworkByPassphrase: Record<string, string> = {
      'Public Global Stellar Network ; September 2015': 'pubnet',
      'Test SDF Network ; September 2015': 'testnet',
    };
    const galexieNetwork =
      galexieNetworkByPassphrase[config.stellarNetworkPassphrase];
    if (!galexieNetwork) {
      throw new Error(
        `Unsupported stellarNetworkPassphrase for Galexie: "${config.stellarNetworkPassphrase}". ` +
          `Supported: ${Object.keys(galexieNetworkByPassphrase).join(', ')}`
      );
    }

    const galexieConfigToml = [
      '[datastore_config]',
      'type = "S3"',
      '',
      '[datastore_config.params]',
      `destination_bucket_path = "${ledgerBucket.bucketName}/"`,
      `region = "${config.awsRegion}"`,
      '',
      '[datastore_config.schema]',
      'ledgers_per_file = 1',
      'files_per_partition = 64000',
      '',
      '[stellar_core_config]',
      `network = "${galexieNetwork}"`,
    ].join('\n');

    /**
     * Entrypoint: write /tmp/config.toml AND materialise the mTLS bundle
     * to /tmp/{cert,key,ca}.pem from the ECS-injected env vars before
     * exec'ing Galexie. `umask 077` ensures the key file is owner-only.
     */
    const galexieCommand = (subcommand: string): string[] => [
      `/bin/bash`,
      `-c`,
      [
        `set -euo pipefail`,
        `umask 077`,
        `cat > /tmp/config.toml <<'TOMLEOF'`,
        galexieConfigToml,
        `TOMLEOF`,
        `printf '%s' "$MTLS_CERT" > /tmp/cert.pem`,
        `printf '%s' "$MTLS_KEY"  > /tmp/key.pem`,
        `printf '%s' "$MTLS_CA"   > /tmp/ca.pem`,
        `exec stellar-galexie ${subcommand} --config-file /tmp/config.toml`,
      ].join('\n'),
    ];

    const taskDefs: ecs.FargateTaskDefinition[] = [];

    const sharedContainerSecrets = {
      MTLS_CERT: ecs.Secret.fromSecretsManager(galexieMtlsSecret, 'cert'),
      MTLS_KEY: ecs.Secret.fromSecretsManager(galexieMtlsSecret, 'key'),
      MTLS_CA: ecs.Secret.fromSecretsManager(galexieMtlsSecret, 'ca'),
    };
    const sharedContainerEnv = {
      CH_DOMAIN: config.chDomainName,
    };

    // ---------------------
    // Galexie Live — Task Definition
    // ---------------------
    const liveTaskDef = new ecs.FargateTaskDefinition(this, 'LiveTaskDef', {
      family: `${config.envName}-galexie-live`,
      cpu: config.galexieCpu,
      memoryLimitMiB: config.galexieMemory,
      ephemeralStorageGiB: config.galexieEphemeralStorage,
      runtimePlatform: {
        cpuArchitecture: ecs.CpuArchitecture.X86_64,
        operatingSystemFamily: ecs.OperatingSystemFamily.LINUX,
      },
    });

    liveTaskDef.addVolume({ name: 'data' });
    liveTaskDef.addVolume({ name: 'tmp' });

    const liveContainer = liveTaskDef.addContainer('Galexie', {
      image: ecs.ContainerImage.fromEcrRepository(repository, galexieImageTag),
      logging: ecs.LogDrivers.awsLogs({
        logGroup: liveLogGroup,
        streamPrefix: 'galexie',
      }),
      entryPoint: galexieCommand('append'),
      workingDirectory: '/data',
      environment: {
        ...sharedContainerEnv,
        START: '62024874',
      },
      secrets: sharedContainerSecrets,
      healthCheck: {
        command: ['CMD-SHELL', 'pgrep -x stellar-core || exit 1'],
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        retries: 3,
        startPeriod: cdk.Duration.seconds(120),
      },
      readonlyRootFilesystem: true,
      stopTimeout: cdk.Duration.seconds(config.galexieStopTimeout),
    });
    liveContainer.addMountPoints(
      { containerPath: '/data', sourceVolume: 'data', readOnly: false },
      { containerPath: '/tmp', sourceVolume: 'tmp', readOnly: false }
    );
    taskDefs.push(liveTaskDef);

    // ---------------------
    // Galexie Live — Fargate Service
    // ---------------------
    // ⚠️  CODEOWNERS-FLAGGED ⚠️
    // `assignPublicIp: true` is LOAD-BEARING (task 0239 Phase 3).
    // Without NAT GW in the post-cutover topology the per-task public
    // IPv4 is Galexie's ONLY egress route — flipping this to `false`
    // breaks ECR pulls, CloudWatch Logs, S3 writes, Stellar overlay
    // peering and the mTLS handshake to Hetzner CH simultaneously.
    // Any PR that toggles this must justify it and provide an
    // alternative egress path (re-introducing NAT GW, VPC endpoints,
    // etc.) — not a drive-by hardening change.
    const liveService = new ecs.FargateService(this, 'LiveService', {
      serviceName: `${config.envName}-galexie-live`,
      cluster,
      taskDefinition: liveTaskDef,
      desiredCount: config.galexieDesiredCount,
      securityGroups: [ecsSecurityGroup],
      vpcSubnets: { subnetType: ec2.SubnetType.PUBLIC },
      assignPublicIp: true,
      platformVersion: ecs.FargatePlatformVersion.LATEST,
      enableExecuteCommand: config.ecsExecEnabled,
      circuitBreaker: { rollback: true },
      minHealthyPercent: 0,
      maxHealthyPercent: 100,
    });
    this.liveService = liveService;

    // ---------------------
    // Galexie Backfill — Task Definition (optional)
    // ---------------------
    if (config.galexieBackfillEnabled) {
      const backfillTaskDef = new ecs.FargateTaskDefinition(
        this,
        'BackfillTaskDef',
        {
          family: `${config.envName}-galexie-backfill`,
          cpu: config.galexieCpu,
          memoryLimitMiB: config.galexieMemory,
          ephemeralStorageGiB: config.galexieEphemeralStorage,
          runtimePlatform: {
            cpuArchitecture: ecs.CpuArchitecture.X86_64,
            operatingSystemFamily: ecs.OperatingSystemFamily.LINUX,
          },
        }
      );

      backfillTaskDef.addVolume({ name: 'data' });
      backfillTaskDef.addVolume({ name: 'tmp' });

      const backfillContainer = backfillTaskDef.addContainer('Galexie', {
        image: ecs.ContainerImage.fromEcrRepository(
          repository,
          galexieImageTag
        ),
        logging: ecs.LogDrivers.awsLogs({
          logGroup: backfillLogGroup,
          streamPrefix: 'galexie',
        }),
        entryPoint: galexieCommand('scan-and-fill'),
        workingDirectory: '/data',
        environment: {
          ...sharedContainerEnv,
          // Soroban mainnet activation ledger (Protocol 20, Feb 20 2024).
          // Overridden per RunTask invocation via container overrides.
          START: '50457424',
          END: '',
        },
        secrets: sharedContainerSecrets,
        healthCheck: {
          command: ['CMD-SHELL', 'pgrep -x stellar-core || exit 1'],
          interval: cdk.Duration.seconds(30),
          timeout: cdk.Duration.seconds(5),
          retries: 3,
          startPeriod: cdk.Duration.seconds(120),
        },
        readonlyRootFilesystem: true,
        stopTimeout: cdk.Duration.seconds(config.galexieStopTimeout),
      });
      backfillContainer.addMountPoints(
        { containerPath: '/data', sourceVolume: 'data', readOnly: false },
        { containerPath: '/tmp', sourceVolume: 'tmp', readOnly: false }
      );
      this.backfillTaskDefinition = backfillTaskDef;
      taskDefs.push(backfillTaskDef);
    }

    // ---------------------
    // IAM Grants
    // ---------------------
    for (const taskDef of taskDefs) {
      // Task role — Galexie reads/writes ledger bucket.
      ledgerBucket.grantReadWrite(taskDef.taskRole);
      taskDef.taskRole.addToPrincipalPolicy(
        new iam.PolicyStatement({
          actions: ['s3:ListBucket'],
          resources: [ledgerBucket.bucketArn],
        })
      );

      // Execution role — ECR pull, plus Secrets Manager read for the
      // mTLS bundle (ECS native secrets injection needs the execution
      // role to fetch the value at task start).
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      repository.grantPull(taskDef.executionRole!);
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      galexieMtlsSecret.grantRead(taskDef.executionRole!);

      // ECS Exec requires SSM perms on the task role (ssmmessages does
      // not support resource-level restrictions — AWS limitation).
      if (config.ecsExecEnabled) {
        taskDef.taskRole.addToPrincipalPolicy(
          new iam.PolicyStatement({
            actions: [
              'ssmmessages:CreateControlChannel',
              'ssmmessages:CreateDataChannel',
              'ssmmessages:OpenControlChannel',
              'ssmmessages:OpenDataChannel',
            ],
            resources: ['*'],
          })
        );
      }
    }

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // ---------------------
    // Outputs
    // ---------------------
    new cdk.CfnOutput(this, 'ClusterName', {
      value: cluster.clusterName,
    });
    new cdk.CfnOutput(this, 'ClusterArn', {
      value: cluster.clusterArn,
    });
    new cdk.CfnOutput(this, 'RepositoryUri', {
      value: repository.repositoryUri,
    });
    new cdk.CfnOutput(this, 'LiveServiceName', {
      value: liveService.serviceName,
    });
    if (this.backfillTaskDefinition) {
      new cdk.CfnOutput(this, 'BackfillTaskDefArn', {
        value: this.backfillTaskDefinition.taskDefinitionArn,
      });
    }
  }
}
