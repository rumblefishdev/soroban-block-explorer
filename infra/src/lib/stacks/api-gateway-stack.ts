import * as cdk from 'aws-cdk-lib';
import * as apigateway from 'aws-cdk-lib/aws-apigateway';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as wafv2 from 'aws-cdk-lib/aws-wafv2';
import type { Construct } from 'constructs';

import type { EnvironmentConfig } from '../types.js';

export interface ApiGatewayStackProps extends cdk.StackProps {
  readonly config: EnvironmentConfig;
  readonly apiFunction: lambda.IFunction;
  readonly wafWebAclArn?: string;
}

/**
 * REST API Gateway for the Soroban Block Explorer.
 *
 * Provides a public HTTP endpoint backed by the API Lambda with:
 * - Lambda proxy integration (all routes forwarded to axum)
 * - Environment-specific throttling (rate + burst limits)
 * - Optional response caching (disabled on staging to save cost)
 * - CORS for SPA browser access
 * - Optional WAF WebACL attachment (wired when task 0035 lands)
 * - API key usage plan for non-browser consumers
 */
export class ApiGatewayStack extends cdk.Stack {
  readonly api: apigateway.RestApi;

  constructor(scope: Construct, id: string, props: ApiGatewayStackProps) {
    super(scope, id, props);

    const { config, apiFunction, wafWebAclArn } = props;

    // ---------------------
    // REST API
    // ---------------------
    const api = new apigateway.LambdaRestApi(this, 'Api', {
      restApiName: `${config.envName}-soroban-explorer-api`,
      handler: apiFunction,
      proxy: true,
      deployOptions: {
        stageName: config.envName,
        throttlingRateLimit: config.apiGatewayThrottleRate,
        throttlingBurstLimit: config.apiGatewayThrottleBurst,
        cacheClusterEnabled: config.apiGatewayCacheEnabled,
        ...(config.apiGatewayCacheEnabled && {
          cacheClusterSize: config.apiGatewayCacheSize,
          cacheTtl: cdk.Duration.seconds(config.apiGatewayCacheTtlMutable),
          cacheDataEncrypted: true,
        }),
      },
      // TODO: restrict to SPA domain when Route 53 is configured (task 0035)
      defaultCorsPreflightOptions: {
        allowOrigins: apigateway.Cors.ALL_ORIGINS,
        allowMethods: ['GET', 'OPTIONS'],
        allowHeaders: ['Content-Type', 'Accept'],
      },
      endpointTypes: [apigateway.EndpointType.REGIONAL],
    });
    this.api = api;

    // ---------------------
    // WAF Attachment
    // ---------------------
    // Task 0035 defines the WAF WebACL. This stack only consumes the
    // ARN and attaches it to the API Gateway stage. Until task 0035
    // is implemented, wafWebAclArn is not passed and this is skipped.
    if (wafWebAclArn) {
      new wafv2.CfnWebACLAssociation(this, 'WafAssociation', {
        resourceArn: api.deploymentStage.stageArn,
        webAclArn: wafWebAclArn,
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
  }
}
