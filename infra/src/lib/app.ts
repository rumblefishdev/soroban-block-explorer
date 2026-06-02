import * as cdk from 'aws-cdk-lib';

import { validateConfig, type EnvironmentConfig } from './types.js';
import { NetworkStack } from './stacks/network-stack.js';
import { LedgerBucketStack } from './stacks/ledger-bucket-stack.js';
import { ComputeStack } from './stacks/compute-stack.js';
import { CloudFrontWafStack } from './stacks/cloudfront-waf-stack.js';
import { DeliveryStack } from './stacks/delivery-stack.js';
import { ApiGatewayStack } from './stacks/api-gateway-stack.js';
import { IngestionStack } from './stacks/ingestion-stack.js';
import { ObservabilityStack } from './stacks/observability-stack.js';
import { CloudWatchStack } from './stacks/cloudwatch-stack.js';
import { HetznerDnsStack } from './stacks/hetzner-dns-stack.js';

export interface CreateAppOptions {
  readonly config: EnvironmentConfig;
  /** Absolute path to the directory containing the root Cargo.toml workspace. */
  readonly cargoWorkspacePath: string;
}

export function createApp({
  config,
  cargoWorkspacePath,
}: CreateAppOptions): void {
  validateConfig(config);

  const app = new cdk.App();

  const env: cdk.Environment = {
    account: process.env['CDK_DEFAULT_ACCOUNT'],
    region: config.awsRegion,
  };

  const prefix = `Explorer-${config.envName}`;

  const network = new NetworkStack(app, `${prefix}-Network`, { env, config });

  const ledgerBucket = new LedgerBucketStack(app, `${prefix}-LedgerBucket`, {
    env,
    config,
  });

  const compute = new ComputeStack(app, `${prefix}-Compute`, {
    env,
    config,
    ledgerBucketArn: ledgerBucket.bucket.bucketArn,
    ledgerBucketName: ledgerBucket.bucket.bucketName,
    cargoWorkspacePath,
  });

  new IngestionStack(app, `${prefix}-Ingestion`, {
    env,
    config,
    vpc: network.vpc,
    ecsSecurityGroup: network.ecsSecurityGroup,
    ledgerBucketArn: ledgerBucket.bucket.bucketArn,
    ledgerBucketName: ledgerBucket.bucket.bucketName,
  });
  // CDK auto-detects dependencies from cross-stack references
  // (vpc, ecsSecurityGroup, bucket ARN/name).

  // CLOUDFRONT-scoped WAF must be created in us-east-1 (AWS requirement);
  // the DeliveryStack distribution (in config.awsRegion) references its ARN
  // via crossRegionReferences.
  let cloudFrontWafArn: string | undefined;
  if (config.enableWaf) {
    const cloudFrontWaf = new CloudFrontWafStack(
      app,
      `${prefix}-CloudFrontWaf`,
      {
        env: { account: env.account, region: 'us-east-1' },
        config,
        crossRegionReferences: true,
      }
    );
    cloudFrontWafArn = cloudFrontWaf.webAclArn;
  }

  new DeliveryStack(app, `${prefix}-Delivery`, {
    env,
    config,
    cloudFrontWafArn,
    crossRegionReferences: true,
  });

  new ObservabilityStack(app, `${prefix}-Observability`, { env, config });

  const apiGateway = new ApiGatewayStack(app, `${prefix}-ApiGateway`, {
    env,
    config,
    apiFunction: compute.apiFunction,
  });
  apiGateway.addDependency(compute);

  const cloudWatch = new CloudWatchStack(app, `${prefix}-CloudWatch`, {
    env,
    config,
    apiFunction: compute.apiFunction,
    processorFunction: compute.processorFunction,
    deadLetterQueue: compute.deadLetterQueue,
    enrichmentDlq: compute.enrichmentDlq,
    enrichmentWorkerFunction: compute.enrichmentWorkerFunction,
    restApi: apiGateway.api,
  });
  cloudWatch.addDependency(apiGateway);

  // HetznerDnsStack only when the env has a real `chDomainName`.
  if (
    !config.chDomainName.includes('PLACEHOLDER') &&
    !config.chDomainName.includes('CHANGE')
  ) {
    new HetznerDnsStack(app, `${prefix}-HetznerDns`, { env, config });
  }

  app.synth();
}
