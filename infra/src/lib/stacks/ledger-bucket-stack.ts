import * as cdk from 'aws-cdk-lib';
import * as s3 from 'aws-cdk-lib/aws-s3';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

export interface LedgerBucketStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
}

/**
 * S3 bucket for LedgerCloseMeta XDR files from Galexie and backfill.
 *
 * No lifecycle rules — files retained indefinitely.
 * Lifecycle can be added later if storage costs become a concern.
 *
 * S3 event notification for Ledger Processor Lambda is configured
 * in ComputeStack (task 0099) via fromBucketAttributes import.
 */
export class LedgerBucketStack extends cdk.Stack {
  readonly bucket: s3.IBucket;

  constructor(scope: Construct, id: string, props: LedgerBucketStackProps) {
    super(scope, id, props);

    const { config } = props;
    const prefix = config.envName;

    this.bucket = new s3.Bucket(this, 'LedgerData', {
      bucketName: `${prefix}-stellar-ledger-data`,
      // SSE-S3 (AES256), never SSE-KMS: the contents are public on-chain XDR,
      // so KMS buys no real confidentiality here — but the high-volume ingest
      // (one Put per ledger + one Get per processor invocation) would make a
      // per-object KMS call, driving an avoidable request bill (task 0278).
      // SSE-S3 still encrypts at rest with zero KMS traffic.
      encryption: s3.BucketEncryption.S3_MANAGED,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      versioned: false,
      // Retention is independent of the (now-fixed) encryption choice:
      // production keeps its data, ephemeral envs auto-clean on teardown.
      removalPolicy: config.kmsEncryption
        ? cdk.RemovalPolicy.RETAIN
        : cdk.RemovalPolicy.DESTROY,
      autoDeleteObjects: !config.kmsEncryption,
    });

    // Tags
    cdk.Tags.of(this).add('Project', 'soroban-block-explorer');
    cdk.Tags.of(this).add('Environment', config.envName);
    cdk.Tags.of(this).add('ManagedBy', 'cdk');

    // Outputs
    new cdk.CfnOutput(this, 'BucketName', {
      value: this.bucket.bucketName,
    });
    new cdk.CfnOutput(this, 'BucketArn', {
      value: this.bucket.bucketArn,
    });
  }
}
