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

  // Cloudflare edge migration (task 0277 / ADR 0048) — origin lockdown.
  // All default false until the Cloudflare zone + certs/secrets exist;
  // enabling them does NOT move DNS, it provisions the AWS-side locks
  // that must be live BEFORE the Cloudflare cutover (task 0277 Step 2).

  /**
   * Provision the AWS-side bootstrap for the Cloudflare migration via CDK
   * (so nothing is created by hand): the Terraform remote-state S3 bucket
   * (versioned, encrypted, private) and the `X-Origin-Secret` in Secrets
   * Manager with a CDK-generated value. Deploy this FIRST — the Terraform
   * backend bucket + the origin secret must exist before `terraform apply`
   * and before the CloudFront secret-header lock is populated. Default false.
   *
   * DEPLOY-ONCE / LEAVE TRUE: both resources are `RETAIN` and become the live
   * Terraform backend + shared secret. Flipping back to false removes the stack
   * from the app and orphans them from CDK (data survives via RETAIN, but the
   * backend is then unmanaged) — so set it true once and keep it.
   *
   * Out of scope (external credential / crypto — cannot be IaC-generated):
   * the Cloudflare API token (paste once via `put-secret-value`) and the
   * mTLS client cert/key (operator `openssl`).
   */
  readonly provisionCloudflareBootstrap: boolean;

  /**
   * Phase 1 of the API mTLS rollout: provision the **versioned** S3
   * truststore bucket (and only that) so the operator can upload the CA
   * bundle PEM (`truststore.pem`) BEFORE mTLS is attached.
   *
   * Split from `enableApiMtls` deliberately: API Gateway validates the
   * truststore S3 object at deploy time, so attaching mTLS against an empty
   * bucket fails. Two-phase rollout: provision bucket → upload PEM →
   * `enableApiMtls`. Default false.
   */
  readonly provisionApiMtlsTruststore: boolean;

  /**
   * Phase 2 — lock the API Gateway custom domain to Cloudflare via mTLS
   * (Path B in [ADR 0048]). When true the `ApiGatewayStack`:
   *  - attaches the S3 **truststore** (the CA bundle that signed
   *    Cloudflare's uploaded Authenticated-Origin-Pulls client cert) to
   *    the REGIONAL custom domain, and
   *  - sets `disableExecuteApiEndpoint=true` so the raw
   *    `execute-api` URL — which bypasses custom-domain mTLS — stops
   *    answering.
   *
   * REQUIRES `provisionApiMtlsTruststore=true` AND the PEM already uploaded
   * (enforced in `validateConfig`). The CA bundle is a **non-secret** PEM
   * uploaded out-of-band; no value is committed. No `crates/api` change
   * (handshake-level reject).
   *
   * ORDERING GOTCHA (task 0277 Step 2): the custom-domain base-path mapping
   * MUST already be live before `disableExecuteApiEndpoint` flips, otherwise
   * the edge 403s itself. The custom domain already exists today, so flipping
   * this on the existing domain is safe — but never enable it before the
   * custom domain serves.
   */
  readonly enableApiMtls: boolean;

  /**
   * Lock the CloudFront `*.cloudfront.net` distribution to Cloudflare via
   * a secret header (Decision 4a in [ADR 0048]). When true a
   * viewer-request CloudFront Function rejects any request whose
   * `x-origin-secret` header does not match the value held in a CloudFront
   * KeyValueStore.
   *
   * The secret VALUE never lives in git or the CloudFormation template —
   * it is populated out-of-band into the KVS (mirroring the
   * `enableBasicAuth` pattern) and set on the Cloudflare side (a Transform
   * Rule) by Terraform. Canonical source is AWS Secrets Manager
   * (`soroban/${envName}/cloudflare/origin-secret`), consistent with the
   * mTLS-bundle precedent (`mtlsSecretNamePrefix`). Closed-by-default: an
   * empty KVS yields 503, never an open distribution.
   *
   * CloudFront allows only ONE viewer-request function per behavior, so
   * this cannot be combined with `enableBasicAuth` as two separate
   * functions — see `validateConfig` (the two are mutually exclusive until
   * a combined guard function lands).
   */
  readonly enableOriginSecretLock: boolean;

  /**
   * Deploy a CloudWatch Synthetics canary that periodically asserts the
   * direct-origin bypass vectors stay **blocked** (return 403) — the
   * recurring synthetic check in task 0277 Step 7 acceptance criteria.
   * It hits the raw `execute-api` URL and the `*.cloudfront.net` domain and
   * alarms (via the existing SNS→Slack topic) if either starts answering
   * 2xx, i.e. the origin lockdown regressed.
   *
   * Enable only AFTER the locks are live (post-cutover): with the locks off
   * those origins legitimately return 2xx, so the canary would alarm
   * continuously (validateConfig warns about this).
   */
  readonly enableOriginLockCanary: boolean;
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

  /**
   * Length of the rolling window (in minutes) over which the Galexie lag alarm
   * sums Ledger Processor invocations. Alarm fires when the sum is 0 — i.e.,
   * no invocation started in the last N minutes. Must exceed the worst-case
   * single-invocation runtime so a long-running batch does not trigger a
   * false positive (current cap = `indexerLambdaTimeout` ≈ 10 min).
   */
  readonly galexieLagMinutes: number;
  /** Error rate threshold (>0.0–1.0) for the Ledger Processor error-rate alarm. */
  readonly processorErrorRateThreshold: number;
  /** API Gateway 5xx error rate % threshold for the 5xx alarm. */
  readonly apiGateway5xxThreshold: number;
  /** Slack workspace ID for AWS Chatbot alarm notifications. */
  readonly slackWorkspaceId: string;
  /** Slack channel ID for AWS Chatbot alarm notifications. */
  readonly slackChannelId: string;

  // Hetzner ClickHouse — mTLS (consumed by ComputeStack, IngestionStack, HetznerDnsStack)

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
    config.mtlsSecretNamePrefix.includes('CHANGE') ||
    config.mtlsSecretNamePrefix.includes('PLACEHOLDER')
  ) {
    errors.push(
      `mtlsSecretNamePrefix missing or placeholder: "${config.mtlsSecretNamePrefix}"`
    );
  }

  // CloudFront allows exactly ONE viewer-request function per behavior.
  // basic auth and the origin-secret lock are each their own
  // viewer-request function, so they cannot both be attached as separate
  // functions. (A combined guard function is a future option — until then
  // they are mutually exclusive.)
  if (config.enableBasicAuth && config.enableOriginSecretLock) {
    errors.push(
      `enableBasicAuth and enableOriginSecretLock are mutually exclusive: ` +
        `CloudFront permits only one viewer-request function per behavior. ` +
        `Pick one (origin-secret lock is the Cloudflare-cutover lock per ADR 0048; ` +
        `basic auth is the temporary pre-launch gate from task 0273), or land a ` +
        `combined guard function first.`
    );
  }

  // API mTLS is a two-phase rollout: the truststore bucket must be
  // provisioned (and the CA PEM uploaded) BEFORE mTLS can be attached —
  // API Gateway validates the truststore S3 object at deploy time.
  if (config.enableApiMtls && !config.provisionApiMtlsTruststore) {
    errors.push(
      `enableApiMtls=true requires provisionApiMtlsTruststore=true: ` +
        `provision the truststore bucket and upload truststore.pem first ` +
        `(API Gateway validates the truststore object at deploy time).`
    );
  }

  // The origin-lock canary only probes vectors whose lock is live; with no lock
  // on it has zero targets and every run fails. Enabling it then is always a
  // mistake — fail at synth rather than page on every run at runtime.
  if (
    config.enableOriginLockCanary &&
    !config.enableApiMtls &&
    !config.enableOriginSecretLock
  ) {
    errors.push(
      `enableOriginLockCanary=true requires at least one origin lock ` +
        `(enableApiMtls and/or enableOriginSecretLock) enabled — otherwise the ` +
        `canary has no targets and every run fails. Enable it only after a lock ` +
        `is live (post-cutover).`
    );
  }

  if (errors.length > 0) {
    throw new Error(
      `Invalid EnvironmentConfig for "${config.envName}":\n  - ${errors.join(
        '\n  - '
      )}`
    );
  }

  // Soft sanity check: an environment with no edge gating at all
  // (no WAF, no basic auth, no origin-secret lock) exposes an
  // unprotected public CloudFront distribution.
  if (
    !config.enableWaf &&
    !config.enableBasicAuth &&
    !config.enableOriginSecretLock
  ) {
    // eslint-disable-next-line no-console
    console.warn(
      `[validateConfig] WARNING: ${config.envName} has enableWaf=false, enableBasicAuth=false ` +
        `and enableOriginSecretLock=false. The CloudFront distribution will be publicly ` +
        `accessible with no gating. If this is intentional, ignore. Otherwise enable one of ` +
        `them in envs/${config.envName}.json.`
    );
  }
}

/** Configuration for the shared CI/CD stack (consumed by CicdStack). */
export interface CicdConfig {
  readonly awsRegion: string;
  /** GitHub org/repo, e.g. "rumblefishdev/soroban-block-explorer" */
  readonly githubRepo: string;
}
