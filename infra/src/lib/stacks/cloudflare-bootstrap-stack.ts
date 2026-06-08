import * as cdk from 'aws-cdk-lib';
import * as s3 from 'aws-cdk-lib/aws-s3';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

export interface CloudflareBootstrapStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
}

/**
 * AWS-side bootstrap for the sorobanscan slice of the Cloudflare edge migration
 * (task 0277 / ADR 0048), so nothing is created by hand. Gated by
 * `config.provisionCloudflareBootstrap` and deployed FIRST (before the first
 * `terraform apply` in `infra/cloudflare/`).
 *
 * Creates ONLY the Terraform remote-state S3 bucket (versioned, encrypted,
 * private, enforced-TLS, RETAIN) — the backend for THIS repo's Cloudflare
 * module (`infra/cloudflare/`), which manages the sorobanscan-specific
 * resources: the `api.sorobanscan.rumblefishdev.com` DNS record + the
 * Cloudflare side of the API origin lock (AOP).
 *
 * Repo split (task 0277 D9/D11): the Cloudflare ZONE (`rumblefishdev.com`),
 * the company DNS records, the zone-level edge rulesets and a SEPARATE
 * TF-state bucket live in the private `rf-domains` repo — NOT here.
 *
 * NOT here either:
 *  - the Cloudflare API token → paste once via `aws secretsmanager
 *    put-secret-value` (it comes from the Cloudflare dashboard)
 *  - the mTLS client cert/key → operator `openssl` (see infra/cloudflare/certs/)
 *
 * No `X-Origin-Secret`: under the repo split the API is locked with mTLS
 * (per-host AOP + API GW mTLS, D12), not a secret header — a secret header
 * would force a cross-repo shared secret + the Transform Rule into rf-domains.
 */
export class CloudflareBootstrapStack extends cdk.Stack {
  constructor(
    scope: Construct,
    id: string,
    props: CloudflareBootstrapStackProps
  ) {
    super(scope, id, props);

    const { config } = props;

    // Terraform remote-state backend bucket for THIS repo's Cloudflare module.
    // RETAIN — never auto-delete state. KMS-encrypted unconditionally: state can
    // carry sensitive attributes (e.g. the mTLS client key if ever managed in
    // TF), independent of the cost/auto-delete knob used elsewhere. The aws/s3
    // managed key permits KMS via the S3 service, so `encrypt = true` in
    // backend.hcl works without an extra key grant.
    const tfStateBucket = new s3.Bucket(this, 'CloudflareTfState', {
      bucketName: `${config.envName}-soroban-explorer-cf-tfstate`,
      versioned: true,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.KMS_MANAGED,
      enforceSSL: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // Bucket name → backend.hcl.
    new cdk.CfnOutput(this, 'CloudflareTfStateBucketName', {
      value: tfStateBucket.bucketName,
    });
  }
}
