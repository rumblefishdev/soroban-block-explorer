import * as cdk from 'aws-cdk-lib';

import { validateConfig, type EnvironmentConfig } from './types.js';
import { NetworkStack } from './stacks/network-stack.js';
import { LedgerBucketStack } from './stacks/ledger-bucket-stack.js';
import { ComputeStack } from './stacks/compute-stack.js';
import { DeliveryStack } from './stacks/delivery-stack.js';
import { ApiGatewayStack } from './stacks/api-gateway-stack.js';
import { IngestionStack } from './stacks/ingestion-stack.js';
import { ObservabilityStack } from './stacks/observability-stack.js';
import { CloudWatchStack } from './stacks/cloudwatch-stack.js';
import { HetznerDnsStack } from './stacks/hetzner-dns-stack.js';
import { CloudflareBootstrapStack } from './stacks/cloudflare-bootstrap-stack.js';

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

  // Edge protection is Cloudflare on the API hostname only (ADR 0048, task
  // 0302). There is no AWS WAF: the CLOUDFRONT-scoped WebACL stack that used to
  // live in us-east-1 and the REGIONAL one in ApiGatewayStack were both removed,
  // which also took the only cross-region reference in this app with them.
  const delivery = new DeliveryStack(app, `${prefix}-Delivery`, {
    env,
    config,
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
    spaDistributionDomainName: delivery.distribution.distributionDomainName,
  });
  cloudWatch.addDependency(apiGateway);

  // AWS-side bootstrap for THIS repo's Cloudflare module (task 0277 / ADR 0048):
  // the Terraform remote-state bucket for infra/cloudflare/. Standalone — no
  // dependency on the other stacks.
  if (config.provisionCloudflareBootstrap) {
    new CloudflareBootstrapStack(app, `${prefix}-CloudflareBootstrap`, {
      env,
      config,
    });
  }

  // HetznerDnsStack only when the env has a real `chDomainName`.
  if (
    !config.chDomainName.includes('PLACEHOLDER') &&
    !config.chDomainName.includes('CHANGE')
  ) {
    new HetznerDnsStack(app, `${prefix}-HetznerDns`, { env, config });
  }

  app.synth();
}
