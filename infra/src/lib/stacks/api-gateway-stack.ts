import * as cdk from 'aws-cdk-lib';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as apigateway from 'aws-cdk-lib/aws-apigateway';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as targets from 'aws-cdk-lib/aws-route53-targets';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as wafv2 from 'aws-cdk-lib/aws-wafv2';
import type { Construct } from 'constructs';

import { WafWebAcl } from '../constructs/waf-web-acl.js';
import { relativeRecordName, type EnvironmentConfig } from '../types.js';

// API Gateway account-level default throttle (rps / burst). Used as the
// "lifted" ceiling when `config.loadTesting` is set, so the per-stage 50/100
// cap stops being the bottleneck during a load test. The account limit still
// backstops genuine abuse, and the WAF rate rule is dropped separately.
const LOAD_TEST_THROTTLE_RATE = 10_000;
const LOAD_TEST_THROTTLE_BURST = 5_000;

export interface ApiGatewayStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly apiFunction: lambda.IFunction;
}

/**
 * REST API Gateway for the Soroban Block Explorer.
 *
 * Provides a public HTTP endpoint backed by the API Lambda with:
 * - Lambda proxy integration (all routes forwarded to axum)
 * - Environment-specific throttling (rate + burst limits)
 * - Optional response caching (disabled on staging to save cost)
 * - CORS for SPA browser access
 * - REGIONAL WAF WebACL with managed rules + rate limit (task 0035)
 * - API key usage plan for non-browser consumers
 *
 * NOTE on WAF: This stack creates its OWN REGIONAL WebACL. The CloudFront
 * distribution (DeliveryStack) has a separate CLOUDFRONT-scoped WebACL
 * with the same rule set. AWS WAF requires distinct ACLs for CLOUDFRONT
 * and REGIONAL scopes — one ACL cannot serve both. See task 0035.
 */
export class ApiGatewayStack extends cdk.Stack {
  readonly api: apigateway.RestApi;

  constructor(scope: Construct, id: string, props: ApiGatewayStackProps) {
    super(scope, id, props);

    const { config, apiFunction } = props;

    // Load-test window (task 0338): lift the per-stage + usage-plan throttle to
    // the account ceiling and drop the WAF per-IP rate rule (below), so a load
    // test measures backend capacity rather than edge throttling. Managed WAF
    // rules, edge_lock, API-key auth, and Lambda↔Hetzner mTLS are untouched.
    const throttleRate = config.loadTesting
      ? LOAD_TEST_THROTTLE_RATE
      : config.apiGatewayThrottleRate;
    const throttleBurst = config.loadTesting
      ? LOAD_TEST_THROTTLE_BURST
      : config.apiGatewayThrottleBurst;

    // ---------------------
    // API mTLS truststore (Cloudflare origin lockdown — ADR 0048, Path B)
    // ---------------------
    // Phase 1, provisioned independently of the mTLS attachment: the
    // operator uploads the CA bundle (`truststore.pem` — the CA that signed
    // Cloudflare's Authenticated-Origin-Pulls client cert) into this bucket
    // BEFORE flipping `enableApiMtls`, because API Gateway validates the
    // truststore S3 object at deploy time. Versioned per the archival
    // guidance and required for CA rotation (mTLS references object versions).
    const mtlsTruststoreBucket = config.provisionApiMtlsTruststore
      ? new s3.Bucket(this, 'ApiMtlsTruststore', {
          bucketName: `${config.envName}-soroban-explorer-api-mtls-truststore`,
          versioned: true,
          blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
          encryption: config.kmsEncryption
            ? s3.BucketEncryption.KMS_MANAGED
            : s3.BucketEncryption.S3_MANAGED,
          enforceSSL: true,
          removalPolicy: cdk.RemovalPolicy.RETAIN,
        })
      : undefined;

    // ---------------------
    // REST API
    // ---------------------
    const api = new apigateway.LambdaRestApi(this, 'Api', {
      restApiName: `${config.envName}-soroban-explorer-api`,
      handler: apiFunction,
      proxy: true,
      // Phase 2: kill the raw execute-api endpoint, which bypasses
      // custom-domain mTLS. Only flips with enableApiMtls.
      ...(config.enableApiMtls && { disableExecuteApiEndpoint: true }),
      deployOptions: {
        stageName: config.envName,
        tracingEnabled: true,
        throttlingRateLimit: throttleRate,
        throttlingBurstLimit: throttleBurst,
        cacheClusterEnabled: config.apiGatewayCacheEnabled,
        ...(config.apiGatewayCacheEnabled && {
          cacheClusterSize: config.apiGatewayCacheSize,
          cacheTtl: cdk.Duration.seconds(config.apiGatewayCacheTtlMutable),
          cacheDataEncrypted: true,
        }),
      },
      defaultCorsPreflightOptions: {
        allowOrigins: [`https://${config.domainName}`],
        // POST is needed for the cross-origin `/auth/session` mint (task 0277
        // paid-API). The SPA on `domainName` calls the API on a different host,
        // so every non-simple request triggers a browser preflight.
        allowMethods: ['GET', 'POST', 'OPTIONS'],
        // `Authorization` (free-tier session JWT) and `x-api-key` (paid tier)
        // are non-safelisted request headers — without them the preflight is
        // rejected and the armed SPA cannot send a Bearer / partner key.
        allowHeaders: ['Content-Type', 'Accept', 'Authorization', 'x-api-key'],
        // Cache the preflight so the browser does NOT re-`OPTIONS` before every
        // `/v1` request (task 0317). Each `/v1` call carries `Authorization`, a
        // non-safelisted header, so without `Access-Control-Max-Age` Chrome
        // re-runs the preflight on every request — an extra edge round-trip per
        // call. The preflight is answered here by API Gateway's MOCK
        // integration (not the Lambda), so the max-age MUST be set on the
        // gateway, not on the axum `CorsLayer`. 1h is within Chromium's 2h cap;
        // CORS config rarely changes, so a stale cached preflight is low-risk.
        maxAge: cdk.Duration.hours(1),
      },
      endpointTypes: [apigateway.EndpointType.REGIONAL],
    });
    this.api = api;

    // ---------------------
    // WAF (REGIONAL) — optional, own WebACL for API Gateway
    // ---------------------
    // Same rule set as the CLOUDFRONT-scoped WebACL in DeliveryStack via
    // the shared WafWebAcl construct. A CLOUDFRONT-scoped WebACL cannot
    // be associated with a REGIONAL resource (API Gateway stage), so two
    // ACLs are required.
    const waf = config.enableWaf
      ? new WafWebAcl(this, 'ApiWaf', {
          scope: 'REGIONAL',
          name: `${config.envName}-soroban-explorer-api`,
          rateLimit: config.apiWafRateLimit,
          // Drop the per-IP rate rule during a load-test window; managed rules stay.
          includeRateLimit: !config.loadTesting,
        })
      : undefined;

    if (waf) {
      new wafv2.CfnWebACLAssociation(this, 'WafAssociation', {
        resourceArn: api.deploymentStage.stageArn,
        webAclArn: waf.webAclArn,
      });
    }

    // ---------------------
    // Usage Plan + API Key
    // ---------------------
    // Optional API key access for non-browser consumers (automation,
    // partner integrations). Browser traffic does not require an API
    // key — the SPA calls the API anonymously.
    //
    // NOTE: In proxy mode, LambdaRestApi creates a greedy {proxy+}
    // resource with apiKeyRequired=false. The usage plan tracks and
    // throttles requests that voluntarily include an x-api-key header,
    // but does not gate access. To enforce API key requirement on
    // specific routes, add non-proxy resources with apiKeyRequired=true.
    const usagePlan = api.addUsagePlan('UsagePlan', {
      name: `${config.envName}-partner-plan`,
      throttle: {
        rateLimit: throttleRate,
        burstLimit: throttleBurst,
      },
      // Daily quota is dropped during a load-test window: with the throttle
      // lifted, a key'd run would otherwise burn the per-day limit in ~1s and
      // 429 for the rest of the day, masking backend capacity. Restored (and
      // the only volumetric cap on key'd traffic) when loadTesting is off.
      ...(config.loadTesting
        ? {}
        : {
            quota: {
              limit: config.apiGatewayPartnerDailyQuota,
              period: apigateway.Period.DAY,
            },
          }),
    });
    usagePlan.addApiStage({ stage: api.deploymentStage });

    const apiKey = api.addApiKey('PartnerApiKey', {
      apiKeyName: `${config.envName}-partner-key`,
    });
    usagePlan.addApiKey(apiKey);

    // ---------------------
    // Custom Domains (Cloudflare migration — task 0277 / ADR 0048)
    // ---------------------
    // mTLS attaches to the CLOUDFLARE custom domain only — the legacy domain
    // stays plain (the SPA hits it directly, with no client cert). validateConfig
    // guarantees the truststore bucket exists when enableApiMtls is set; the
    // extra guard keeps this type-safe.
    const mtlsConfig =
      config.enableApiMtls && mtlsTruststoreBucket
        ? {
            mtls: {
              bucket: mtlsTruststoreBucket,
              key: 'truststore.pem',
            },
            securityPolicy: apigateway.SecurityPolicy.TLS_1_2,
          }
        : {};

    // Legacy custom domain (api.sorobanscan.rumblefish.dev) + Route 53 — plain
    // TLS. Live during the migration so the SPA keeps working; retire with one
    // deploy via enableLegacyApiDomain=false after the cutover is verified.
    if (config.enableLegacyApiDomain) {
      const certificate = acm.Certificate.fromCertificateArn(
        this,
        'ApiCertificate',
        config.apiCertificateArn
      );

      const apiDomain = api.addDomainName('ApiDomain', {
        domainName: config.apiDomainName,
        certificate,
        endpointType: apigateway.EndpointType.REGIONAL,
      });

      const hostedZone = route53.HostedZone.fromHostedZoneAttributes(
        this,
        'HostedZone',
        {
          hostedZoneId: config.hostedZoneId,
          zoneName: config.hostedZoneName,
        }
      );

      // recordName must be RELATIVE to the hosted zone — see
      // relativeRecordName() in types.ts.
      const apiRecordName = relativeRecordName(
        config.apiDomainName,
        config.hostedZoneName
      );

      new route53.ARecord(this, 'ApiARecord', {
        zone: hostedZone,
        recordName: apiRecordName,
        target: route53.RecordTarget.fromAlias(
          new targets.ApiGatewayDomain(apiDomain)
        ),
      });

      new route53.AaaaRecord(this, 'ApiAaaaRecord', {
        zone: hostedZone,
        recordName: apiRecordName,
        target: route53.RecordTarget.fromAlias(
          new targets.ApiGatewayDomain(apiDomain)
        ),
      });

      new cdk.CfnOutput(this, 'ApiCustomDomain', {
        value: `https://${config.apiDomainName}`,
      });
    }

    // Cloudflare-fronted custom domain (api-sorobanscan.rumblefishdev.com) — a
    // SECOND custom domain on the same API, with NO Route 53 record (Cloudflare
    // is authoritative). mTLS-lockable. Its regional alias target is output so it
    // can be fed into the Cloudflare module's api_origin_target.
    if (config.enableCloudflareApiDomain) {
      const cfCertificate = acm.Certificate.fromCertificateArn(
        this,
        'CloudflareApiCertificate',
        config.cloudflareApiCertificateArn
      );

      const cfApiDomain = api.addDomainName('CloudflareApiDomain', {
        domainName: config.cloudflareApiDomainName,
        certificate: cfCertificate,
        endpointType: apigateway.EndpointType.REGIONAL,
        ...mtlsConfig,
      });

      new cdk.CfnOutput(this, 'CloudflareApiRegionalTarget', {
        value: cfApiDomain.domainNameAliasDomainName,
      });
      new cdk.CfnOutput(this, 'CloudflareApiCustomDomain', {
        value: `https://${config.cloudflareApiDomainName}`,
      });
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
    new cdk.CfnOutput(this, 'ApiEndpoint', {
      value: api.url,
    });
    if (mtlsTruststoreBucket) {
      new cdk.CfnOutput(this, 'ApiMtlsTruststoreBucket', {
        value: mtlsTruststoreBucket.bucketName,
      });
    }
    if (waf) {
      new cdk.CfnOutput(this, 'ApiWafWebAclArn', {
        value: waf.webAclArn,
      });
    }
  }
}
