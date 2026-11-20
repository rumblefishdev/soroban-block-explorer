import * as cdk from 'aws-cdk-lib';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import type { Construct } from 'constructs';

import { apiHubBackendHeadersFunctionCode } from '../cloudfront-functions/api-hub-backend-headers.js';
import type { EnvironmentConfig } from '../types.js';

/** What the stack already has that the hub's behaviors share. */
export interface ApiHubShared {
  readonly responseHeadersPolicy: cloudfront.IResponseHeadersPolicy;
}

/**
 * `/api/hub/*` (task 0680): the API hub's backend, a Lambda function URL
 * (`config.apiHub.backendHost`). Never cached; the viewer's cookies, query
 * string and four headers are forwarded. CloudFront stamps the origin with
 * `X-Hub-Origin-Secret` (a Secrets Manager dynamic reference, read again only
 * when `originSecretRevision` changes the template) so the backend can refuse
 * a call that did not come through here.
 *
 * Call only with `config.apiHub` set.
 */
export function apiHubBackendBehavior(
  scope: Construct,
  config: EnvironmentConfig,
  shared: ApiHubShared
): cloudfront.BehaviorOptions {
  const hub = config.apiHub;
  if (!hub) throw new Error('apiHubBackendBehavior needs config.apiHub');

  const secret = secretsmanager.Secret.fromSecretNameV2(
    scope,
    'ApiHubOriginSecret',
    hub.originSecretName
  );

  const originRequestPolicy = new cloudfront.OriginRequestPolicy(
    scope,
    'ApiHubOriginRequestPolicy',
    {
      originRequestPolicyName: `${config.envName}-soroban-explorer-api-hub-backend`,
      comment: 'The API hub backend: all cookies and query, four headers',
      cookieBehavior: cloudfront.OriginRequestCookieBehavior.all(),
      queryStringBehavior: cloudfront.OriginRequestQueryStringBehavior.all(),
      headerBehavior: cloudfront.OriginRequestHeaderBehavior.allowList(
        'Origin',
        'Content-Type',
        'Accept',
        'CloudFront-Viewer-Address'
      ),
    }
  );

  // See the function's own comment: AWS says an origin custom header
  // overwrites a viewer's, but says nothing for `CloudFront-Viewer-Address`.
  const headersFunction = new cloudfront.Function(
    scope,
    'ApiHubBackendHeadersFunction',
    {
      functionName: `${config.envName}-soroban-explorer-api-hub-backend-headers`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      code: cloudfront.FunctionCode.fromInline(
        apiHubBackendHeadersFunctionCode()
      ),
    }
  );

  return {
    origin: new origins.HttpOrigin(hub.backendHost, {
      protocolPolicy: cloudfront.OriginProtocolPolicy.HTTPS_ONLY,
      readTimeout: cdk.Duration.seconds(30),
      customHeaders: {
        'X-Hub-Origin-Secret': secret
          .secretValueFromJson('current')
          .unsafeUnwrap(),
        'X-Hub-Origin-Revision': hub.originSecretRevision,
      },
    }),
    viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.HTTPS_ONLY,
    allowedMethods: cloudfront.AllowedMethods.ALLOW_ALL,
    cachePolicy: cloudfront.CachePolicy.CACHING_DISABLED,
    originRequestPolicy,
    responseHeadersPolicy: shared.responseHeadersPolicy,
    functionAssociations: [
      {
        function: headersFunction,
        eventType: cloudfront.FunctionEventType.VIEWER_REQUEST,
      },
    ],
  };
}
