import * as cdk from 'aws-cdk-lib';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import type { Construct } from 'constructs';

import { apiHubBackendHeadersFunctionCode } from '../cloudfront-functions/api-hub-backend-headers.js';
import { apiHubRoutingFunctionCode } from '../cloudfront-functions/api-hub-routing.js';
import type { HostMove } from '../cloudfront-functions/legacy-host-redirect.js';
import type { EnvironmentConfig } from '../types.js';

/** What the stack already has that the behaviors under `/api` share. */
export interface ApiHubShared {
  readonly responseHeadersPolicy: cloudfront.IResponseHeadersPolicy;
  /** The settings `responseHeadersPolicy` is built from. */
  readonly securityHeaders: cloudfront.ResponseSecurityHeadersBehavior;
  /** The main SPA's host move: the hub lives on the new host only. */
  readonly legacyMove: HostMove | undefined;
  /** The main SPA's default behavior, with the main behaviors' function. */
  readonly explorerPage: cloudfront.BehaviorOptions;
  /** The API SPA bucket, which holds the hub's bundle under `api/`. */
  readonly apiSpaOrigin: cloudfront.IOrigin;
  /** A short-TTL behavior on the API SPA bucket that runs `fn`. */
  readonly apiSpaBehavior: (
    fn: cloudfront.IFunction
  ) => cloudfront.BehaviorOptions;
}

/**
 * The behaviors under `/api` with the API hub's pages (`apiHub.pages`, task
 * 0680, the start), as the hub's spec has them:
 *
 * - `/api`, `/api/*`, `/api/prices-api` and `/api/prices-api/*`: one function
 *   (`api-hub-routing.ts`) over the bucket: the hub's page for its addresses,
 *   the bundle's files as asked, a redirect for the rest. The two Prices
 *   behaviors add `X-Robots-Tag: noindex`.
 * - `/api/assets/*`: the bundle's hashed files, cached as long as the
 *   explorer's own `/assets/*`, with the main behaviors' function (the old
 *   host's `301`) as `/assets/*` has it.
 * - `/api/hub/*`: the backend (`apiHubBackendBehavior`).
 *
 * The bucket's behaviors come before the backend's, so the bucket's origin
 * keeps the number, and the origin access control, it has with the pages off.
 * `pricesPortalOnLegacyHost` is refused with the pages (`validateConfig`): the
 * hub's function sends the old host to the new one, the Prices pages too.
 *
 * Call only with `config.apiHub` set.
 */
export function apiHubPageBehaviors(
  scope: Construct,
  config: EnvironmentConfig,
  shared: ApiHubShared
): Record<string, cloudfront.BehaviorOptions> {
  const routingFunction = new cloudfront.Function(
    scope,
    'ApiHubRoutingFunction',
    {
      functionName: `${config.envName}-soroban-explorer-api-hub-routing`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      code: cloudfront.FunctionCode.fromInline(
        apiHubRoutingFunctionCode(shared.legacyMove)
      ),
    }
  );

  // The Prices API's page under `/api/prices-api/` stays out of search
  // results: the hub's own pages are the ones to index.
  const noindexHeadersPolicy = new cloudfront.ResponseHeadersPolicy(
    scope,
    'ApiHubNoindexHeadersPolicy',
    {
      responseHeadersPolicyName: `${config.envName}-soroban-explorer-headers-noindex`,
      securityHeadersBehavior: shared.securityHeaders,
      customHeadersBehavior: {
        customHeaders: [
          { header: 'X-Robots-Tag', value: 'noindex', override: true },
        ],
      },
    }
  );

  const page = shared.apiSpaBehavior(routingFunction);
  const pricesPage = { ...page, responseHeadersPolicy: noindexHeadersPolicy };

  return {
    '/api/assets/*': {
      ...shared.explorerPage,
      origin: shared.apiSpaOrigin,
      cachePolicy: cloudfront.CachePolicy.CACHING_OPTIMIZED,
    },
    // `/api/prices-api/*` does not match bare `/api/prices-api`, which the
    // function sends to `/api/prices-api/`.
    '/api/prices-api': pricesPage,
    '/api/prices-api/*': pricesPage,
    '/api/hub/*': apiHubBackendBehavior(scope, config, shared),
    // `/api/*` does not match bare `/api` either.
    '/api': page,
    '/api/*': page,
  };
}

/**
 * `/api/hub/*` (task 0680): the API hub's backend, a Lambda function URL
 * (`config.apiHub.backendHost`). Never cached; the viewer's cookies, query
 * string and four headers are forwarded. CloudFront stamps the origin with
 * `X-Hub-Origin-Secret` (a Secrets Manager dynamic reference, which
 * CloudFormation reads again whenever it updates the distribution, for any
 * reason; raising `originSecretRevision` makes it) so the backend can refuse
 * a call that did not come through here.
 *
 * The distribution turns every 403 and 404 into the explorer's `index.html`
 * (200), and that cannot be limited to this path: a 403 or 404 from the
 * function URL (a missing invoke permission, a wrong `backendHost`) shows as
 * the explorer's page. Call the function URL directly to tell.
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

  // By the secret's name, the form CloudFormation documents for a dynamic
  // reference (`fromSecretNameV2` would put a partial ARN in it).
  const secretValue = cdk.SecretValue.secretsManager(hub.originSecretName, {
    jsonField: 'current',
  });

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
        'X-Hub-Viewer-Ip'
      ),
    }
  );

  // Writes `X-Hub-Viewer-Ip` and drops a viewer's own secret and revision
  // (see the function's own comment).
  const headersFunction = new cloudfront.Function(
    scope,
    'ApiHubBackendHeadersFunction',
    {
      functionName: `${config.envName}-soroban-explorer-api-hub-backend-headers`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      code: cloudfront.FunctionCode.fromInline(
        apiHubBackendHeadersFunctionCode(shared.legacyMove)
      ),
    }
  );

  return {
    origin: new origins.HttpOrigin(hub.backendHost, {
      protocolPolicy: cloudfront.OriginProtocolPolicy.HTTPS_ONLY,
      readTimeout: cdk.Duration.seconds(30),
      customHeaders: {
        'X-Hub-Origin-Secret': secretValue.unsafeUnwrap(),
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
