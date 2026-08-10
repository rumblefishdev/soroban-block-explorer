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

  /**
   * Load-test window switch (task 0338). When `true`, the API Gateway stack
   * lifts the volumetric DDoS protections so a load test measures backend
   * capacity instead of edge throttling: the per-stage + usage-plan throttle
   * is raised to the account ceiling. It used to drop the REGIONAL WAF's per-IP
   * rate-based rule as well; that WebACL no longer exists (ADR 0048, task 0302),
   * so the throttle is now the only thing this switch touches. Everything else
   * stays: `edge_lock` (X-Edge-Secret), API-key auth, and the Lambda↔Hetzner
   * mTLS are untouched.
   *
   * DANGER: this removes the rate protections on the PUBLIC production API.
   * Only flip `true` for a coordinated test window, then flip back. Never
   * commit `true`. `validateConfig` emits a loud warning when it is set.
   */
  readonly loadTesting: boolean;

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
   * Enable CloudFront Function basic auth on the SPA distribution.
   * Production should leave this false.
   */
  readonly enableBasicAuth: boolean;

  // Cloudflare edge migration (task 0277 / ADR 0048) — origin lockdown.
  // All default false until the Cloudflare zone + certs/secrets exist;
  // enabling them does NOT move DNS, it provisions the AWS-side locks
  // that must be live BEFORE the Cloudflare cutover (task 0277 Step 2).

  /**
   * Provision the AWS-side bootstrap for THIS repo's Cloudflare module via CDK
   * (so nothing is created by hand): the Terraform remote-state S3 bucket
   * (versioned, encrypted, private) that backs `infra/cloudflare/`. Deploy this
   * FIRST — the backend bucket must exist before the first `terraform apply`.
   *
   * DEPLOY-ONCE / LEAVE TRUE: the bucket is `RETAIN` and becomes the live
   * Terraform backend. Flipping back to false removes the stack from the app and
   * orphans the bucket from CDK (data survives via RETAIN, but it is then
   * unmanaged) — so set it true once and keep it.
   *
   * Scope note (task 0277 D9/D11): this is the bucket for the **sorobanscan**
   * slice only (api DNS record + AOP origin lock). The Cloudflare zone, company
   * DNS, zone-level rulesets and a SEPARATE state bucket live in the private
   * `rf-domains` repo. Default false.
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
   * Keep the legacy API custom domain (`apiDomainName`,
   * api.sorobanscan.rumblefish.dev) + its Route 53 A/AAAA records. Keep TRUE
   * during the Cloudflare migration so the live SPA path keeps working; flip to
   * false (one deploy) to RETIRE it after the cutover to the Cloudflare host is
   * verified. Plain TLS, no mTLS — the SPA hits it directly.
   */
  readonly enableLegacyApiDomain: boolean;

  /**
   * Add the Cloudflare-fronted API custom domain (`cloudflareApiDomainName`,
   * api.sorobanscan.rumblefishdev.com) on the REGIONAL API — a SECOND custom
   * domain alongside the legacy one, with **no Route 53 record** (Cloudflare is
   * authoritative for that zone). Its regional alias target is emitted as the
   * `CloudflareApiRegionalTarget` output → feed it into the Cloudflare module's
   * `api_origin_target`. mTLS attaches HERE (not the legacy domain) when
   * `enableApiMtls`. Default false.
   */
  readonly enableCloudflareApiDomain: boolean;

  /** Cloudflare-fronted API custom domain, e.g. "api.sorobanscan.rumblefishdev.com". */
  readonly cloudflareApiDomainName: string;

  /**
   * ACM cert ARN for `cloudflareApiDomainName`. Same region as `awsRegion`
   * (REGIONAL custom domain). DNS-validated via the rumblefishdev.com zone.
   */
  readonly cloudflareApiCertificateArn: string;

  /**
   * Phase 1 of the secret-header origin lock (task 0277 / ADR 0048): provision
   * the CDK-generated `EdgeSecret` in Secrets Manager (and only that). Split from
   * `enableEdgeSecretLock` so the value can be copied into the Cloudflare
   * Transform Rule (rf-domains) BEFORE the Lambda starts requiring the header.
   * Default false.
   */
  readonly provisionEdgeSecret: boolean;

  /**
   * Phase 2 — arm the origin lock. When true the API Lambda gets the
   * `EDGE_SECRET` env (the provisioned secret's value); the Lambda's `edge_lock`
   * middleware then rejects any request (except `/health`) lacking a matching
   * `X-Edge-Secret` — i.e. any request that did not pass through Cloudflare.
   *
   * REQUIRES `provisionEdgeSecret=true` AND the Cloudflare Transform Rule
   * already injecting the matching value (rf-domains `enable_edge_secret`).
   * Arming before the edge stamps the header would 403 even legitimate
   * Cloudflare traffic. Default false.
   */
  readonly enableEdgeSecretLock: boolean;

  /**
   * Phase 1 of the paid-API access layer (task 0277; docs/paid-api/
   * plan-platne-api.md): provision its Secrets Manager secrets — a CDK-generated
   * `JwtSecret` (HS256 session signing key) plus operator-populated
   * `TurnstileSecret` (the Cloudflare Turnstile *secret* key) and `ApiKeysSecret`
   * (comma-separated paid-tier keys). Split from `enableAuthLayer` because the
   * Lambda env resolves secret values at DEPLOY time, so the operator must
   * overwrite the Turnstile/API-keys placeholders BEFORE arming. Default false.
   */
  readonly provisionAuthSecrets: boolean;

  /**
   * Phase 2 — arm the access layer. Injects `JWT_SECRET` / `TURNSTILE_SECRET` /
   * `API_KEYS` into the API Lambda; the `auth` gate then requires a valid paid
   * `X-API-Key` or a free session JWT (from Turnstile) on data routes (401 else).
   *
   * REQUIRES `provisionAuthSecrets=true` AND the Turnstile secret already
   * populated AND the SPA already sending sessions (Turnstile → Bearer);
   * arming before the SPA does so would 401 real users. Default false.
   */
  readonly enableAuthLayer: boolean;

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
  /**
   * Ephemeral-storage utilization % threshold for the Galexie captive-core
   * disk alarm. Baseline is ~30% (captive-core's BucketList = current ledger
   * state); 60 gives long lead time to plan a disk bump before a merge/catchup
   * spike hits the "No space left on device" ceiling (incident 2026-07-01/02).
   */
  readonly galexieEphemeralUtilizationThreshold: number;
  /**
   * Minimum total impact (USD) a cost anomaly must reach before Cost Anomaly
   * Detection notifies the alarm topic (task 0449/0455). The account's
   * baseline is a few USD/day, so single-digit USD of unexplained daily
   * deviation is already the July-incident shape — a step change in one
   * service's spend that previously went unnoticed for three weeks.
   */
  readonly costAnomalyAlertThresholdUsd: number;
  // Slack workspace + channel IDs are NOT in env config — they are
  // deployment-specific identifiers kept out of the (public) repo and sourced
  // at deploy time from SSM Parameter Store (see CloudWatchStack).

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
    !(config.costAnomalyAlertThresholdUsd > 0) ||
    config.costAnomalyAlertThresholdUsd > 1000
  ) {
    errors.push(
      `costAnomalyAlertThresholdUsd must be > 0 and <= 1000 USD, got: ${config.costAnomalyAlertThresholdUsd}`
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
    config.galexieEphemeralUtilizationThreshold <= 0 ||
    config.galexieEphemeralUtilizationThreshold > 100
  ) {
    errors.push(
      `galexieEphemeralUtilizationThreshold must be between 0 and 100, got: ${config.galexieEphemeralUtilizationThreshold}`
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

  // Edge-secret origin lock is two-phase: the secret must exist before the
  // Lambda is armed to require it.
  if (config.enableEdgeSecretLock && !config.provisionEdgeSecret) {
    errors.push(
      `enableEdgeSecretLock=true requires provisionEdgeSecret=true: provision ` +
        `the EdgeSecret and copy its value into the Cloudflare Transform Rule ` +
        `(rf-domains) before arming the Lambda.`
    );
  }

  // Auth layer is two-phase: the secrets (esp. the operator-populated Turnstile
  // key) must exist before the Lambda env resolves them at deploy.
  if (config.enableAuthLayer && !config.provisionAuthSecrets) {
    errors.push(
      `enableAuthLayer=true requires provisionAuthSecrets=true: provision the ` +
        `JWT/Turnstile/API-keys secrets and populate the Turnstile secret first.`
    );
  }

  // mTLS now attaches to the Cloudflare custom domain (not the legacy one), so
  // it makes no sense without that domain present.
  if (config.enableApiMtls && !config.enableCloudflareApiDomain) {
    errors.push(
      `enableApiMtls=true requires enableCloudflareApiDomain=true: mTLS attaches ` +
        `to the Cloudflare API custom domain, not the legacy one.`
    );
  }

  // The Cloudflare API domain needs a real same-region cert (not the placeholder).
  if (
    config.enableCloudflareApiDomain &&
    (!config.cloudflareApiDomainName ||
      config.cloudflareApiCertificateArn.includes('REPLACE'))
  ) {
    errors.push(
      `enableCloudflareApiDomain=true requires cloudflareApiDomainName and a real ` +
        `cloudflareApiCertificateArn (got a placeholder).`
    );
  }

  // The origin-lock canary only probes vectors whose lock is live; with no lock
  // on it has zero targets and every run fails. Enabling it then is always a
  // mistake — fail at synth rather than page on every run at runtime.
  if (
    config.enableOriginLockCanary &&
    !config.enableApiMtls &&
    !config.enableOriginSecretLock &&
    !config.enableEdgeSecretLock
  ) {
    errors.push(
      `enableOriginLockCanary=true requires at least one origin lock ` +
        `(enableApiMtls, enableOriginSecretLock, and/or enableEdgeSecretLock) ` +
        `enabled — otherwise the canary has no targets and every run fails. ` +
        `Enable it only after a lock is live (post-cutover).`
    );
  }

  if (errors.length > 0) {
    throw new Error(
      `Invalid EnvironmentConfig for "${config.envName}":\n  - ${errors.join(
        '\n  - '
      )}`
    );
  }

  // Soft sanity check: an environment with no gating on the SPA distribution
  // (no basic auth, no origin-secret lock) exposes it unfiltered. On production
  // that is the accepted end state (ADR 0048, task 0302) — the distribution
  // serves static edge-cached files from a private S3 origin — so the warning
  // is informational, not a defect.
  if (!config.enableBasicAuth && !config.enableOriginSecretLock) {
    // eslint-disable-next-line no-console
    console.warn(
      `[validateConfig] NOTE: ${config.envName} has enableBasicAuth=false and ` +
        `enableOriginSecretLock=false, and there is no AWS WAF. The CloudFront ` +
        `distribution is publicly accessible with no edge gating. On production this ` +
        `is intentional; enable one of them in envs/${config.envName}.json if it is not.`
    );
  }

  // Loud, deliberately hard-to-miss warning: a load-test deploy strips the
  // volumetric DDoS protections off the public API (task 0338). Allowed (the
  // test runs against prod), but it must never ship unnoticed or stay on.
  if (config.loadTesting) {
    // eslint-disable-next-line no-console
    console.warn(
      `\n========================================================================\n` +
        `[validateConfig] !!! LOAD-TEST MODE ACTIVE on "${config.envName}" !!!\n` +
        `  API Gateway throttle is raised to the account ceiling. That throttle\n` +
        `  is the only volumetric protection on the origin, so the public API has\n` +
        `  NO rate protection in this deploy. Only valid for a coordinated test\n` +
        `  window — set loadTesting=false in envs/${config.envName}.json and\n` +
        `  redeploy as soon as the run is done. Do NOT commit loadTesting=true.\n` +
        `========================================================================\n`
    );
  }
}

/** Configuration for the shared CI/CD stack (consumed by CicdStack). */
export interface CicdConfig {
  readonly awsRegion: string;
  /** GitHub org/repo, e.g. "rumblefishdev/soroban-block-explorer" */
  readonly githubRepo: string;
}
