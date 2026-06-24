import * as cdk from 'aws-cdk-lib';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as lambdaEventSources from 'aws-cdk-lib/aws-lambda-event-sources';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as s3n from 'aws-cdk-lib/aws-s3-notifications';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as sns from 'aws-cdk-lib/aws-sns';
import * as subs from 'aws-cdk-lib/aws-sns-subscriptions';
import * as sqs from 'aws-cdk-lib/aws-sqs';
import * as ssm from 'aws-cdk-lib/aws-ssm';
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
    // Ledger ingest queue (task 0241 — S3 → SQS → Lambda)
    // ---------------------
    // S3 `ObjectCreated` lands here; the indexer's SQS event-source-mapping
    // drains it. A burst while the reserved concurrency slot is busy buffers
    // in the queue (visible `ApproximateNumberOfMessages`, multi-day
    // retention) rather than in Lambda's opaque ~6h async-invoke buffer.
    // After `maxReceiveCount` failed deliveries a message moves to `dlq`,
    // from which SQS redrive-to-source recovers it once the cause is fixed.
    const ingestQueue = new sqs.Queue(this, 'LedgerIngestQueue', {
      queueName: `${config.envName}-ledger-ingest`,
      // MUST be ≥ the function timeout, else SQS redelivers a doorbell the
      // indexer is still legitimately processing (a reconcile can run up to
      // the full timeout). timeout + 60 s margin.
      visibilityTimeout: cdk.Duration.seconds(config.indexerLambdaTimeout + 60),
      retentionPeriod: cdk.Duration.days(DLQ_RETENTION_DAYS),
      deadLetterQueue: {
        queue: dlq,
        // Higher than the usual 3: with `indexerLambdaConcurrency = 1` the SQS
        // ESM over-polls and gets throttled (429) — those redeliveries bump
        // ReceiveCount without being real failures. 10 absorbs the throttle
        // churn so a genuinely processable ledger is not DLQ'd by accident.
        maxReceiveCount: 10,
      },
    });

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

    // Origin-lock shared secret (task 0277 / ADR 0048, secret-header variant).
    // CDK-generated. Two-phase, like the mTLS truststore, so the secret can exist
    // (to be copied into the Cloudflare Transform Rule) BEFORE the Lambda starts
    // requiring the header:
    //   phase 1  provisionEdgeSecret  → create the secret (Lambda NOT yet armed)
    //   (then)   copy value → rf-domains Transform Rule injects X-Edge-Secret
    //   phase 2  enableEdgeSecretLock → set EDGE_SECRET env → middleware enforces
    // RETAIN so rotation is deliberate.
    const edgeSecret = config.provisionEdgeSecret
      ? new secretsmanager.Secret(this, 'EdgeSecret', {
          secretName: `soroban/${config.envName}/cloudflare/edge-secret`,
          description:
            'X-Edge-Secret shared by the Cloudflare Transform Rule (rf-domains) and the API Lambda origin-lock middleware (task 0277).',
          generateSecretString: {
            passwordLength: 48,
            excludePunctuation: true,
          },
          removalPolicy: cdk.RemovalPolicy.RETAIN,
        })
      : undefined;

    // Paid-API access-layer secrets (task 0277; docs/paid-api/plan-platne-api.md).
    // Phase 1 (provisionAuthSecrets): create them. JwtSecret is CDK-generated (a
    // session-signing key, never copied out). TurnstileSecret + ApiKeysSecret are
    // created with a generated placeholder the operator OVERWRITES with the real
    // Turnstile secret key / the comma-separated paid keys (the Turnstile
    // placeholder simply fails siteverify until then; the api-keys placeholder is
    // one unknown key = no real paid access). RETAIN. Env wired in phase 2 below.
    const authSecrets = config.provisionAuthSecrets
      ? {
          jwt: new secretsmanager.Secret(this, 'JwtSecret', {
            secretName: `soroban/${config.envName}/auth/jwt-secret`,
            description:
              'HS256 signing key for free-tier session JWTs (task 0277 paid-API).',
            generateSecretString: {
              passwordLength: 64,
              excludePunctuation: true,
            },
            removalPolicy: cdk.RemovalPolicy.RETAIN,
          }),
          turnstile: new secretsmanager.Secret(this, 'TurnstileSecret', {
            secretName: `soroban/${config.envName}/auth/turnstile-secret`,
            description:
              'Cloudflare Turnstile SECRET key — operator overwrites with the value from the Turnstile widget (task 0277).',
            generateSecretString: { passwordLength: 40 },
            removalPolicy: cdk.RemovalPolicy.RETAIN,
          }),
          apiKeys: new secretsmanager.Secret(this, 'ApiKeysSecret', {
            secretName: `soroban/${config.envName}/auth/api-keys`,
            description:
              'Comma-separated paid-tier API keys — operator overwrites (task 0277).',
            generateSecretString: {
              passwordLength: 40,
              excludePunctuation: true,
            },
            removalPolicy: cdk.RemovalPolicy.RETAIN,
          }),
        }
      : undefined;

    const apiFunction = new RustFunction(this, 'ApiFunction', {
      functionName: `${config.envName}-soroban-explorer-api`,
      manifestPath: cargoWorkspacePath,
      binaryName: 'api',
      ...sharedLambdaProps,
      logGroup: apiLogGroup,
      memorySize: config.apiLambdaMemory,
      timeout: cdk.Duration.seconds(config.apiLambdaTimeout),
      // Build the `swagger-ui` opt-in feature (task 0243): serves an
      // interactive OpenAPI explorer at `/api-docs` so the ClickHouse read
      // paths can be exercised against the live API. Adds ~12 MB of embedded
      // assets to the binary (cold-start load only; the 256 MB Lambda has
      // ample headroom). The spec JSON at `/api-docs-json` is always on,
      // feature or not.
      bundling: {
        cargoLambdaFlags: ['--features', 'swagger-ui'],
      },
      environment: {
        ...sharedEnv,
        AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH: 'true',
        // Secret re-resolution lever (task 0277). The secret env vars below are
        // CloudFormation `{{resolve:secretsmanager:...}}` dynamic references —
        // CFN only re-resolves them when the TEMPLATE changes. After rotating a
        // secret VALUE (api-keys, turnstile, edge, jwt) in Secrets Manager,
        // BUMP this string and redeploy so the Lambda picks up the new value;
        // otherwise `cdk deploy` reports "no changes" and keeps the stale env.
        SECRETS_REVISION: '2',
        // OpenAPI `servers` block (Swagger "Try it out" target). Must be the
        // Cloudflare-fronted host so Swagger calls are same-origin with the docs
        // page AND traverse the edge (X-Edge-Secret) instead of hitting the
        // edge-locked legacy domain (task 0277). Falls back to the legacy domain
        // for envs without the Cloudflare domain.
        API_BASE_URL: `https://${
          config.cloudflareApiDomainName ?? config.apiDomainName
        }`,
        // CORS allow-origin for the cross-origin SPA. API Gateway answers only
        // the OPTIONS preflight; the actual responses come from the Lambda and
        // need Access-Control-Allow-Origin (task 0277). `domainName` is the SPA host.
        CORS_ALLOW_ORIGIN: `https://${config.domainName}`,
        MTLS_SECRET_NAME: apiSecretName,
        // Origin lock (task 0277), phase 2: arm the middleware by injecting the
        // shared secret as EDGE_SECRET. The Lambda's edge_lock middleware then
        // rejects any request (except /health) lacking a matching X-Edge-Secret —
        // i.e. that did not pass through Cloudflare. Resolved by CloudFormation at
        // deploy time (dynamic reference). Only when BOTH the secret is
        // provisioned AND the lock is enabled; otherwise unset = middleware no-op.
        ...(config.enableEdgeSecretLock &&
          edgeSecret && {
            EDGE_SECRET: edgeSecret.secretValue.unsafeUnwrap(),
          }),
        // Paid-API access layer (task 0277), phase 2: arm by injecting the
        // session-signing key, Turnstile secret, and paid-key allowlist. The
        // `auth` gate enforces only when JWT_SECRET is present. Resolved at
        // deploy time — flip enableAuthLayer only AFTER the Turnstile secret is
        // populated AND the SPA sends sessions, else real users get 401.
        ...(config.enableAuthLayer &&
          authSecrets && {
            JWT_SECRET: authSecrets.jwt.secretValue.unsafeUnwrap(),
            TURNSTILE_SECRET: authSecrets.turnstile.secretValue.unsafeUnwrap(),
            API_KEYS: authSecrets.apiKeys.secretValue.unsafeUnwrap(),
          }),
        // Transitional PG placeholder. The API binary still constructs a sqlx
        // PG pool at boot unconditionally (crates/api/src/main.rs); it uses
        // `connect_lazy`, so this URL is NEVER dialed for CH-routed modules.
        // RDS has been removed (ADR 0047), so without *some* value here
        // `db::secrets::resolve_or_env()` returns MissingEnvVar and boot panics
        // (502 on every route). This keeps boot healthy. The not-yet-migrated
        // PG modules (Assets/NFTs/LiquidityPools/Search)
        // still error on query until they get a CH path — expected. Removing
        // this hack needs the PG pool made optional at boot (deferred
        // follow-up); until then a `cdk deploy` MUST keep it, or it regresses
        // prod to the boot panic.
        DATABASE_URL: 'postgres://disabled:disabled@127.0.0.1:5432/disabled',
        // ClickHouse read-path cutover (task 0243 / ADR 0047). Each
        // `API_DATASOURCE_<MODULE>=ch` flips that handler module from the
        // sqlx/PG path to the `clickhouse` path; absence (or any non-`ch`
        // value) keeps PG. CH host (`CH_DOMAIN`) + the mTLS bundle
        // (`MTLS_SECRET_NAME`, granted below) are already wired, so a flag
        // is all it takes to opt a module in. Rollback per module = delete
        // its line and redeploy.
        //
        // Enabled here: modules whose CH read path is merged on `develop` —
        // Network (pilot, PR #221), Ledgers (PR #226), Transactions
        // (PR #235), Accounts (PR #236), Contracts (PR #237), LiquidityPools
        // (task 0243; PRs #246/#248/#250 — all 5 LP endpoints on CH, validated
        // live on prod), Assets (task 0243; PR #260), NFTs (task 0243; PR #274).
        // The remaining module (Search) has no CH path yet.
        //
        // PRECONDITIONS before this deploy goes live (see PR checklist):
        //   1. Hetzner CH is live-ingesting at chain head (not frozen) —
        //      otherwise the API serves stale data.
        //   2. Operator CH smoke passed for the enabled modules. The list
        //      read paths now read in primary-key order (no FINAL-over-
        //      partition scan) and `contract_ids` is ops-only, so the polled
        //      paths are cheap. The remaining read-heavy path is the contract
        //      / op_type *filter* (Statement B/C driver scans a partition by a
        //      non-PK column, ~2e8 rows) — bounded and user-initiated; watch
        //      the api_reader `read_rows` quota (CH Code: 201) rather than the
        //      memory limit.
        API_DATASOURCE_NETWORK: 'ch',
        API_DATASOURCE_LEDGERS: 'ch',
        API_DATASOURCE_TRANSACTIONS: 'ch',
        API_DATASOURCE_ACCOUNTS: 'ch',
        API_DATASOURCE_CONTRACTS: 'ch',
        API_DATASOURCE_LIQUIDITY_POOLS: 'ch',
        // Assets list + detail were orphaned on the PG default after the
        // CH cutover (PG is no longer the live store), so both endpoints
        // served nothing. The CH path (`assets/queries_ch.rs`) mirrors the
        // contracts/accounts modules already on CH. Operator must run the
        // CH read-rows smoke (per `queries_ch.rs` header) before relying on
        // this in prod.
        API_DATASOURCE_ASSETS: 'ch',
        // NFTs read path (task 0243 NFT slice, PR #274). Same precondition as
        // Assets: prod CH must carry `nft_enrichment` (else NULL name/media —
        // ~84% enriched per task 0306) and the operator CH read-rows smoke must
        // pass (`nfts/queries_ch.rs`) before relying on this in prod.
        API_DATASOURCE_NFTS: 'ch',
        // Search read path (task 0318) — the last PG-only module. On PG the
        // endpoint 504'd (~29s) since PG was disabled in prod (ADR 0047).
        // `search/queries_ch.rs` fires classification-gated, concurrent
        // per-entity buckets (no full-table hash joins → no CH Code 241).
        // Preconditions before relying on this in prod: prod CH must carry
        // `soroban_contract_metadata` + `nft_enrichment` (else contract/NFT
        // name search returns empty, not an error), and the operator CH
        // read-rows/memory smoke must pass (bounded full-scans: asset_code
        // substring + contract-name/nft metadata).
        API_DATASOURCE_SEARCH: 'ch',
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

    // ---------------------
    // Ledger events fan-out topic (task 0306)
    // ---------------------
    // A second tenant (prices-api, same AWS account) needs the same
    // `ObjectCreated` doorbells. S3 allows only ONE destination per overlapping
    // `event + suffix`, so we fan out through SNS: the bucket publishes to this
    // topic, and each consumer subscribes its own SQS queue. prices-api owns the
    // subscribe side via its own deploy-role IAM (no cross-account policy needed
    // while we share an account); it reads the topic ARN from SSM below.
    const ledgerEventsTopic = new sns.Topic(this, 'LedgerEventsTopic', {
      topicName: `${config.envName}-ledger-events`,
    });

    // S3 `ObjectCreated` → SNS (was `SqsDestination(ingestQueue)`). Always wired
    // (not gated on concurrency) so a paused indexer
    // (`indexerLambdaConcurrency = 0`) still captures events durably in the
    // queue instead of dropping them on the floor. `SnsDestination` auto-adds
    // the topic policy letting S3 publish.
    ledgerBucket.addEventNotification(
      s3.EventType.OBJECT_CREATED,
      new s3n.SnsDestination(ledgerEventsTopic),
      { suffix: '.xdr.zst' }
    );

    // SNS → indexer's ingest queue. Our indexer treats the SQS message as a
    // content-free doorbell — `SqsMessage` (crates/indexer/src/handler/mod.rs)
    // deserializes only `messageId` and ignores the body — so the SNS envelope
    // vs raw-event body shape does NOT affect ingestion either way.
    // `rawMessageDelivery: true` is kept because (a) it leaves the SQS body
    // byte-identical to the legacy direct `S3 → SQS` event and (b) it is the
    // shape the prices-api consumer expects (it DOES read the S3 object key from
    // the body). The indexer's ESM and `messageId` extraction are unchanged
    // regardless of this flag.
    ledgerEventsTopic.addSubscription(
      new subs.SqsSubscription(ingestQueue, { rawMessageDelivery: true })
    );

    // ---------------------
    // Cross-team SSM hand-off (task 0306)
    // ---------------------
    // prices-api's CDK reads these at ITS deploy time (never at Lambda runtime)
    // to subscribe its own queue to the topic and locate the ledger bucket. The
    // `/platform/{env}/*` namespace is the contract its stack already references
    // (distinct from our own `/soroban-explorer/{env}/*` keys). The network
    // passphrase is the public mainnet/testnet value, not a secret.
    const platformParams: Record<string, string> = {
      'ledger-events-topic-arn': ledgerEventsTopic.topicArn,
      'stellar-ledger-data-bucket-name': ledgerBucketName,
      'stellar-ledger-data-bucket-arn': ledgerBucketArn,
      'ch-domain': config.chDomainName,
      'stellar-network-passphrase': config.stellarNetworkPassphrase,
    };
    for (const [key, value] of Object.entries(platformParams)) {
      new ssm.StringParameter(this, `Platform-${key}`, {
        parameterName: `/platform/${config.envName}/${key}`,
        stringValue: value,
      });
    }

    // SQS → indexer event-source-mapping. Gated on concurrency so a
    // `concurrency = 0` pause leaves messages waiting in the queue with no
    // poller (no `maxReceiveCount` churn → no DLQ spam) until resume.
    // `reportBatchItemFailures` lets the handler fail just the offending
    // message; the rest of the batch is acknowledged and deleted.
    if (config.indexerLambdaConcurrency > 0) {
      processorFunction.addEventSource(
        new lambdaEventSources.SqsEventSource(ingestQueue, {
          batchSize: 1,
          reportBatchItemFailures: true,
          // No `maxConcurrency`: AWS requires ESM MaximumConcurrency ≤ the
          // function's reserved concurrency AND its minimum is 2, so it is
          // unsettable while `indexerLambdaConcurrency = 1`. Execution is
          // capped to one-at-a-time by reserved concurrency alone; the ESM may
          // over-poll and get throttled, which the queue's `maxReceiveCount`
          // (10) absorbs without false-DLQ'ing a processable ledger.
        })
      );
    }
    ingestQueue.grantConsumeMessages(processorFunction);

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
