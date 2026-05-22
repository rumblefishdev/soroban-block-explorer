/**
 * Environment-specific configuration for the CDK infrastructure.
 *
 * Post-task-0239 the AWS side is stateless: Lambdas run out-of-VPC,
 * Galexie runs in a public subnet with a per-task public IP, and the
 * data plane lives on the Hetzner-hosted ClickHouse box reached over
 * mTLS. There is no RDS, no NAT Gateway, no private subnet.
 *
 * Production is the only supported AWS environment until product
 * explicitly asks to bring staging back (see task 0249 archive notes).
 */
export interface EnvironmentConfig {
  readonly envName: 'production';
  readonly awsRegion: string;

  // Network (consumed by NetworkStack)
  readonly vpcCidr: string;
  readonly availabilityZones: readonly string[];

  // KMS (consumed by LedgerBucketStack, IngestionStack)
  readonly kmsEncryption: boolean;

  // Compute (consumed by ComputeStack)
  readonly apiLambdaMemory: number;
  readonly apiLambdaTimeout: number;
  readonly indexerLambdaMemory: number;
  readonly indexerLambdaTimeout: number;
  readonly indexerLambdaConcurrency: number;
  readonly indexerLambdaRetryAttempts: number;

  /** Memory for the type-1 enrichment worker Lambda (task 0191). */
  readonly enrichmentWorkerLambdaMemory: number;
  /** Per-invocation timeout for the type-1 enrichment worker Lambda. */
  readonly enrichmentWorkerLambdaTimeout: number;
  /**
   * Reserved concurrency for the type-1 enrichment worker. Kept low
   * (1–2) to be polite to issuer servers. `0` disables the worker.
   */
  readonly enrichmentWorkerLambdaConcurrency: number;

  // Ingestion — ECS Fargate (consumed by IngestionStack)

  /** Fargate CPU units for Galexie tasks (256, 512, 1024, 2048, 4096). */
  readonly galexieCpu: number;
  /** Fargate memory in MiB for Galexie tasks. Must be compatible with CPU — see https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-cpu-memory-error.html */
  readonly galexieMemory: number;
  /** Ephemeral storage in GiB (21–200). Captive Core needs local disk for ledger catchup. */
  readonly galexieEphemeralStorage: number;
  /** Desired count for the Galexie live service (typically 1 — single writer). */
  readonly galexieDesiredCount: number;
  /** Stellar network passphrase. Determines which network Galexie connects to. */
  readonly stellarNetworkPassphrase: string;
  /** CloudWatch Logs retention in days for ECS log groups. */
  readonly ecsLogRetentionDays: number;
  /** Graceful shutdown timeout in seconds. ECS waits this long after SIGTERM before SIGKILL. */
  readonly galexieStopTimeout: number;
  /** Enable ECS Exec (shell access into containers via SSM). Adds ssmmessages IAM permissions. */
  readonly ecsExecEnabled: boolean;
  /**
   * ECR image tag for Galexie container. Defaults to "latest" until CI/CD
   * pipeline (task 0039) is implemented — once available, each deploy will
   * set this to a git SHA for immutable, reproducible deployments.
   */
  readonly galexieImageTag: string;
  /** Whether to create the backfill task definition. Not every environment needs backfill. */
  readonly galexieBackfillEnabled: boolean;

  // API Gateway (consumed by ApiGatewayStack)

  /** Sustained requests per second before API Gateway returns 429. */
  readonly apiGatewayThrottleRate: number;
  /** Maximum concurrent requests allowed in a short burst above the rate limit. */
  readonly apiGatewayThrottleBurst: number;
  /** Whether to provision a dedicated cache cluster (Memcached) on the stage. false = no cluster, no cost. */
  readonly apiGatewayCacheEnabled: boolean;
  /** Cache cluster size in GB. String because AWS API accepts '0.5', '1.6', '6.1', etc. */
  readonly apiGatewayCacheSize: string;
  /** Cache TTL (seconds) for immutable endpoints (e.g. /transactions/{hash}). */
  readonly apiGatewayCacheTtlImmutable: number;
  /** Cache TTL (seconds) for mutable endpoints (e.g. /transactions?limit=20). Used as the stage-level default. */
  readonly apiGatewayCacheTtlMutable: number;
  /** Daily request quota for partner API key usage plans. */
  readonly apiGatewayPartnerDailyQuota: number;

  // Delivery (consumed by DeliveryStack + ApiGatewayStack)

  /** Frontend SPA domain, e.g. "sorobanscan.rumblefish.dev". */
  readonly domainName: string;
  /** API custom domain, e.g. "api.sorobanscan.rumblefish.dev". */
  readonly apiDomainName: string;
  /** Existing Route 53 hosted zone ID for sorobanscan.rumblefish.dev (global). */
  readonly hostedZoneId: string;
  /** Hosted zone name, e.g. "sorobanscan.rumblefish.dev". */
  readonly hostedZoneName: string;
  /**
   * ACM wildcard certificate ARN backing the CloudFront viewer cert.
   *
   * **Must be in `us-east-1` regardless of `awsRegion`** — CloudFront
   * only accepts viewer certs from us-east-1. Don't "fix" this by
   * matching the cert region to the workload region; CloudFront will
   * reject the deploy.
   */
  readonly cloudFrontCertificateArn: string;
  /**
   * ACM wildcard certificate ARN backing the API Gateway custom domain.
   * Must be in the same region as `awsRegion` (API Gateway regional
   * endpoint requires same-region cert).
   */
  readonly apiCertificateArn: string;
  /**
   * Provision WAF WebACLs (one CLOUDFRONT-scoped on the distribution,
   * one REGIONAL-scoped on the API Gateway stage).
   */
  readonly enableWaf: boolean;
  /**
   * Enable CloudFront Function basic auth on the SPA distribution.
   * Production should leave this false.
   */
  readonly enableBasicAuth: boolean;
  /** Per-IP request limit over a 5-minute window for the CloudFront WAF. */
  readonly cloudFrontWafRateLimit: number;
  /** Per-IP request limit over a 5-minute window for the API Gateway WAF. */
  readonly apiWafRateLimit: number;

  // Observability — X-Ray (consumed by ObservabilityStack)

  /** X-Ray sampling rate (0.0–1.0). Lower in production to reduce cost. */
  readonly xraySamplingRate: number;
  /** X-Ray reservoir size — fixed traces/sec guaranteed before sampling kicks in. */
  readonly xrayReservoirSize: number;

  // Observability — CloudWatch alarms (consumed by CloudWatchStack)

  /** Minutes of zero Ledger Processor invocations before the Galexie lag alarm fires. */
  readonly galexieLagMinutes: number;
  /** Error rate threshold (>0.0–1.0) for the Ledger Processor error-rate alarm. */
  readonly processorErrorRateThreshold: number;
  /** API Gateway 5xx error rate % threshold for the 5xx alarm. */
  readonly apiGateway5xxThreshold: number;
  /** Slack workspace ID for AWS Chatbot alarm notifications. */
  readonly slackWorkspaceId: string;
  /** Slack channel ID for AWS Chatbot alarm notifications. */
  readonly slackChannelId: string;

  // Hetzner ClickHouse — mTLS (consumed by ComputeStack, IngestionStack, MigrationStack, PartitionStack, HetznerDnsStack)

  /**
   * FQDN that Route 53 maps to the Hetzner ClickHouse box. Used as both
   * the mTLS endpoint hostname by AWS-side workloads (Lambda, Galexie)
   * and the LE HTTP-01 challenge target by Caddy on the box.
   *
   * The IPv4 target is read from SSM Parameter Store at
   * `/soroban/${envName}/ch-ip` by `HetznerDnsStack`.
   */
  readonly chDomainName: string;
  /**
   * Secret-name prefix in AWS Secrets Manager for mTLS client cert
   * bundles. Each AWS service (Lambda, Galexie) gets its own secret at
   * `${mtlsSecretNamePrefix}/<cn>` containing `{cert, key, ca}` (per
   * task 0240; identity is asserted by Caddy CN→user map, no
   * ch_user/ch_password in the bundle).
   *
   * Example: `soroban/production/mtls` → service secrets live at
   * `soroban/production/mtls/lambda-api-production`,
   * `soroban/production/mtls/galexie-production`, etc.
   *
   * Stacks construct the full ARN at synth time using
   * `cdk.Stack.of(this).{account,region}`.
   */
  readonly mtlsSecretNamePrefix: string;
}

/**
 * Returns the record name relative to the hosted zone, suitable for
 * `recordName` on `route53.ARecord` / `AaaaRecord`.
 *
 * CDK Route 53 record constructs concatenate `recordName` with the
 * hosted zone name unless `recordName` ends with a trailing dot. Passing
 * a full FQDN like `staging.sorobanscan.rumblefish.dev` (no trailing dot)
 * against a zone `sorobanscan.rumblefish.dev` therefore produces a
 * broken record `staging.sorobanscan.rumblefish.dev.sorobanscan.rumblefish.dev`.
 *
 * This helper strips the zone suffix so callers always get a relative
 * label. For an apex record (`fqdn === zoneName`) it returns the zone
 * name itself, which CDK accepts as the apex.
 */
export function relativeRecordName(fqdn: string, zoneName: string): string {
  if (fqdn === zoneName) {
    return zoneName;
  }
  const suffix = `.${zoneName}`;
  if (!fqdn.endsWith(suffix)) {
    throw new Error(
      `relativeRecordName: "${fqdn}" is not within zone "${zoneName}"`
    );
  }
  return fqdn.slice(0, -suffix.length);
}

/**
 * Validates an EnvironmentConfig at synth time. Throws on missing or
 * placeholder values rather than letting `cdk synth`/`cdk deploy` fail
 * deep inside CloudFormation with cryptic errors.
 */
export function validateConfig(config: EnvironmentConfig): void {
  const errors: string[] = [];

  // CloudFront cert must be in us-east-1 regardless of awsRegion.
  if (
    !/^arn:aws:acm:us-east-1:\d{12}:certificate\//.test(
      config.cloudFrontCertificateArn
    )
  ) {
    errors.push(
      `cloudFrontCertificateArn must be a us-east-1 ACM certificate ARN (CloudFront requirement), got: "${config.cloudFrontCertificateArn}"`
    );
  }
  // API Gateway regional cert must be in the workload region.
  const apiCertPattern = new RegExp(
    `^arn:aws:acm:${config.awsRegion}:\\d{12}:certificate/`
  );
  if (!apiCertPattern.test(config.apiCertificateArn)) {
    errors.push(
      `apiCertificateArn must be an ACM certificate ARN in "${config.awsRegion}" (API Gateway regional endpoint), got: "${config.apiCertificateArn}"`
    );
  }
  if (!/^Z[A-Z0-9]+$/.test(config.hostedZoneId)) {
    errors.push(
      `hostedZoneId must be a Route 53 hosted zone ID (e.g. "Z1234ABCD"), got: "${config.hostedZoneId}"`
    );
  }
  if (!config.hostedZoneName || config.hostedZoneName.includes('CHANGE')) {
    errors.push(
      `hostedZoneName missing or placeholder: "${config.hostedZoneName}"`
    );
  }
  if (!config.domainName || config.domainName.includes('CHANGE')) {
    errors.push(`domainName missing or placeholder: "${config.domainName}"`);
  }
  if (!config.apiDomainName || config.apiDomainName.includes('CHANGE')) {
    errors.push(
      `apiDomainName missing or placeholder: "${config.apiDomainName}"`
    );
  }
  if (config.cloudFrontWafRateLimit < 100) {
    errors.push(
      `cloudFrontWafRateLimit must be >= 100 (AWS WAF minimum), got: ${config.cloudFrontWafRateLimit}`
    );
  }
  if (config.apiWafRateLimit < 100) {
    errors.push(
      `apiWafRateLimit must be >= 100 (AWS WAF minimum), got: ${config.apiWafRateLimit}`
    );
  }
  if (config.xraySamplingRate < 0 || config.xraySamplingRate > 1) {
    errors.push(
      `xraySamplingRate must be between 0.0 and 1.0, got: ${config.xraySamplingRate}`
    );
  }
  if (
    !Number.isInteger(config.xrayReservoirSize) ||
    config.xrayReservoirSize < 0
  ) {
    errors.push(
      `xrayReservoirSize must be a non-negative integer, got: ${config.xrayReservoirSize}`
    );
  }

  if (
    !Number.isInteger(config.galexieLagMinutes) ||
    config.galexieLagMinutes < 1 ||
    config.galexieLagMinutes > 100
  ) {
    errors.push(
      `galexieLagMinutes must be an integer between 1 and 100, got: ${config.galexieLagMinutes}`
    );
  }
  if (
    config.processorErrorRateThreshold <= 0 ||
    config.processorErrorRateThreshold > 1
  ) {
    errors.push(
      `processorErrorRateThreshold must be > 0 and <= 1, got: ${config.processorErrorRateThreshold}`
    );
  }
  if (
    config.apiGateway5xxThreshold <= 0 ||
    config.apiGateway5xxThreshold > 100
  ) {
    errors.push(
      `apiGateway5xxThreshold must be between 0 and 100, got: ${config.apiGateway5xxThreshold}`
    );
  }
  if (
    !config.slackWorkspaceId ||
    config.slackWorkspaceId.includes('CHANGE_ME')
  ) {
    errors.push(
      `slackWorkspaceId missing or placeholder: "${config.slackWorkspaceId}"`
    );
  }
  if (!config.slackChannelId || config.slackChannelId.includes('CHANGE_ME')) {
    errors.push(
      `slackChannelId missing or placeholder: "${config.slackChannelId}"`
    );
  }

  // Hetzner DNS — per-env (consumed by HetznerDnsStack).
  if (!config.chDomainName) {
    errors.push(`chDomainName missing`);
  } else if (
    config.chDomainName.includes('CHANGE') ||
    config.chDomainName.includes('PLACEHOLDER')
  ) {
    errors.push(`chDomainName placeholder rejected: "${config.chDomainName}"`);
  }
  if (
    !config.mtlsSecretNamePrefix ||
    config.mtlsSecretNamePrefix.includes('CHANGE')
  ) {
    errors.push(
      `mtlsSecretNamePrefix missing or placeholder: "${config.mtlsSecretNamePrefix}"`
    );
  }

  if (errors.length > 0) {
    throw new Error(
      `Invalid EnvironmentConfig for "${config.envName}":\n  - ${errors.join(
        '\n  - '
      )}`
    );
  }

  // Soft sanity check: an environment with neither WAF nor basic auth
  // exposes an unprotected public CloudFront distribution.
  if (!config.enableWaf && !config.enableBasicAuth) {
    // eslint-disable-next-line no-console
    console.warn(
      `[validateConfig] WARNING: ${config.envName} has both enableWaf=false and enableBasicAuth=false. ` +
        `The CloudFront distribution will be publicly accessible with no gating. ` +
        `If this is intentional, ignore. Otherwise enable one of them in envs/${config.envName}.json.`
    );
  }
}

/** Configuration for the shared CI/CD stack (consumed by CicdStack). */
export interface CicdConfig {
  readonly awsRegion: string;
  /** GitHub org/repo, e.g. "rumblefishdev/soroban-block-explorer" */
  readonly githubRepo: string;
}
