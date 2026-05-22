import * as cdk from 'aws-cdk-lib';
import * as cr from 'aws-cdk-lib/custom-resources';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import { RustFunction } from 'cargo-lambda-cdk';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';
import { mtlsSecretArn, secretsManagerLayerArn } from '../mtls.js';

export interface MigrationStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly cargoWorkspacePath: string;
}

/**
 * Database migration stack for the Soroban Block Explorer.
 *
 * Runs DB migrations as a CloudFormation custom resource before
 * application Lambdas are deployed. Migration failure blocks the deploy.
 *
 * Post-task-0239: Lambda runs OUT-of-VPC, reaches Hetzner CH over
 * mTLS using the `db-migrate-<env>` client cert in Secrets Manager.
 * The actual schema-engine swap (PG sqlx → CH HTTP migrations) is
 * tracked separately; this stack only wires the new transport.
 */
export class MigrationStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: MigrationStackProps) {
    super(scope, id, props);

    const { config, cargoWorkspacePath } = props;

    const secretName = `${config.mtlsSecretNamePrefix}/lambda-migration-${config.envName}`;

    const secretsExtensionLayer = lambda.LayerVersion.fromLayerVersionArn(
      this,
      'SecretsExtensionLayer',
      secretsManagerLayerArn(this.region)
    );

    // ---------------------
    // Migration Lambda
    // ---------------------
    const migrationFn = new RustFunction(this, 'MigrationFunction', {
      functionName: `${config.envName}-soroban-explorer-db-migrate`,
      manifestPath: cargoWorkspacePath,
      binaryName: 'db-migrate',
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

    migrationFn.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['secretsmanager:GetSecretValue'],
        resources: [mtlsSecretArn(this, secretName)],
      })
    );

    // ---------------------
    // CDK Provider + Custom Resource
    // ---------------------
    // Provider wraps the Lambda as a CloudFormation custom resource handler.
    // On Create/Update it invokes the Lambda; if it fails, CloudFormation
    // rolls back the stack and ComputeStack is never updated.
    const provider = new cr.Provider(this, 'MigrationProvider', {
      onEventHandler: migrationFn,
      logRetention: logs.RetentionDays.ONE_MONTH,
    });

    // migrationVersion uses Date.now() to force a re-invocation on every
    // deployment. This is safe because the migrations applied by the
    // Lambda are idempotent — already-applied migrations are skipped.
    new cdk.CustomResource(this, 'RunMigrations', {
      serviceToken: provider.serviceToken,
      properties: {
        migrationVersion: Date.now().toString(),
      },
    });

    // ---------------------
    // Tags
    // ---------------------
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');
  }
}
