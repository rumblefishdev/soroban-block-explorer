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
        throttlingRateLimit: config.apiGatewayThrottleRate,
        throttlingBurstLimit: config.apiGatewayThrottleBurst,
        cacheClusterEnabled: config.apiGatewayCacheEnabled,
        ...(config.apiGatewayCacheEnabled && {
          cacheClusterSize: config.apiGatewayCacheSize,
          cacheTtl: cdk.Duration.seconds(config.apiGatewayCacheTtlMutable),
          cacheDataEncrypted: true,
        }),
      },
      defaultCorsPreflightOptions: {
        allowOrigins: [`https://${config.domainName}`],
        allowMethods: ['GET', 'OPTIONS'],
        allowHeaders: ['Content-Type', 'Accept'],
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
        rateLimit: config.apiGatewayThrottleRate,
        burstLimit: config.apiGatewayThrottleBurst,
      },
      quota: {
        limit: config.apiGatewayPartnerDailyQuota,
        period: apigateway.Period.DAY,
      },
    });
    usagePlan.addApiStage({ stage: api.deploymentStage });

    const apiKey = api.addApiKey('PartnerApiKey', {
      apiKeyName: `${config.envName}-partner-key`,
    });
    usagePlan.addApiKey(apiKey);

    // ---------------------
    // Custom Domain + Route 53
    // ---------------------
    const certificate = acm.Certificate.fromCertificateArn(
      this,
      'ApiCertificate',
      config.apiCertificateArn
    );

    const apiDomain = api.addDomainName('ApiDomain', {
      domainName: config.apiDomainName,
      certificate,
      endpointType: apigateway.EndpointType.REGIONAL,
      // Phase 2: attach the mTLS truststore so the REGIONAL custom domain
      // rejects any client cert not signed by our CA at the handshake.
      // validateConfig guarantees the bucket exists when enableApiMtls is set;
      // the extra guard keeps this type-safe.
      ...(config.enableApiMtls &&
        mtlsTruststoreBucket && {
          mtls: {
            bucket: mtlsTruststoreBucket,
            key: 'truststore.pem',
          },
          securityPolicy: apigateway.SecurityPolicy.TLS_1_2,
        }),
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
    new cdk.CfnOutput(this, 'ApiCustomDomain', {
      value: `https://${config.apiDomainName}`,
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
