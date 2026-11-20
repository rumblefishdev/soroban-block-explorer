import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import type { Construct } from 'constructs';

import {
  apiHubBackendBehavior,
  type ApiHubShared,
} from './api-hub-behaviors.js';
import { apiHubRedirectFunctionCode } from '../cloudfront-functions/api-hub-redirect.js';
import { apiSpaRoutingFunctionCode } from '../cloudfront-functions/api-spa-routing.js';
import type { HostMove } from '../cloudfront-functions/legacy-host-redirect.js';
import type { EnvironmentConfig } from '../types.js';

/** What the portals' behaviors take from the stack, beyond the hub's. */
export interface ApiPortalShared extends ApiHubShared {
  /** The Prices portal's host move: `legacyMove`, or back while it stays on the old host. */
  readonly portalMove: HostMove | undefined;
  /** The Basic Auth credentials the portals' gates read. */
  readonly basicAuthKvs: cloudfront.IKeyValueStore;
  /** The main SPA's default behavior: the portal hub at `/api/` is a route of it. */
  readonly explorerPage: cloudfront.BehaviorOptions;
  /** A short-TTL behavior on the API SPA bucket that runs `fn`. */
  readonly apiSpaBehavior: (
    fn: cloudfront.IFunction
  ) => cloudfront.BehaviorOptions;
}

/**
 * The behaviors under `/api` before the API hub's pages (tasks 0519, 0608,
 * 0660, 0670): the Prices portal at `/api/prices-api`+`/api/prices-api/*`,
 * the x402 portal at `/api/x402-facilitator`+`/api/x402-facilitator/*`, the
 * portal hub at `/api/` (a route of the main SPA; bare `/api` falls to the
 * default behavior), the API hub's backend at `/api/hub/*` with
 * `config.apiHub` (task 0680), and a redirect for every other `/api/*`.
 *
 * Each portal is an independently-built SPA in the API SPA bucket, under its
 * own key prefix, with its own CloudFront Function handling SPA routing
 * fallback (bare `<prefix>` redirect, extensionless paths rewritten to
 * `<prefix>/index.html`): the distribution's `errorResponses` can't do this
 * for them, since custom error pages are resolved through the _default_
 * behavior's origin, not the originating one.
 */
export function apiPortalBehaviors(
  scope: Construct,
  config: EnvironmentConfig,
  shared: ApiPortalShared
): Record<string, cloudfront.BehaviorOptions> {
  const { apiSpaBehavior, basicAuthKvs, legacyMove, portalMove } = shared;

  // `/api/prices-api` + `/api/prices-api/*` routing function — always
  // created. SPA routing (bare-`/api/prices-api` redirect, extensionless-path
  // fallback to `/api/prices-api/index.html`) must work whether or not auth is on; the auth
  // check is folded in only when `enableApiSpaBasicAuth` is set, since
  // CloudFront allows only one viewer-request function per behavior.
  const apiSpaKvs = config.enableApiSpaBasicAuth ? basicAuthKvs : undefined;
  const apiSpaFunction = new cloudfront.Function(
    scope,
    'ApiSpaRoutingFunction',
    {
      functionName: `${config.envName}-soroban-explorer-api-spa-routing`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      ...(apiSpaKvs && { keyValueStore: apiSpaKvs }),
      code: cloudfront.FunctionCode.fromInline(
        apiSpaRoutingFunctionCode({
          prefix: '/api/prices-api',
          ...(apiSpaKvs && {
            auth: {
              kvsId: apiSpaKvs.keyValueStoreId,
              key: 'auth-token',
              realm: 'API',
            },
          }),
          move: portalMove,
        })
      ),
    }
  );

  // `/api/x402-facilitator` + `/api/x402-facilitator/*` (task 0670): the
  // same routing for the API key portal of stellar-x402. Its page is on
  // `domainName` only, so the old host answers `301` (`legacyMove`), whatever
  // the Prices portal does.
  //
  // The gate reads a key of its own, so its password is not the main site's
  // or the Prices portal's. Write it BEFORE the deploy that creates these
  // behaviors, and the page is never served without it (until the key
  // exists, the answer is `503`):
  //
  //   KVS_ARN=$(aws cloudfront list-key-value-stores --query "KeyValueStoreList.Items[?Name=='production-soroban-explorer-basic-auth'].ARN" --output text)
  //   ETAG=$(aws cloudfront-keyvaluestore describe-key-value-store --kvs-arn "$KVS_ARN" --query "ETag" --output text)
  //   read -rs PW
  //   TOKEN=$(printf '%s' "<user>:$PW" | base64 | tr -d '\n')
  //   aws cloudfront-keyvaluestore put-key --kvs-arn "$KVS_ARN" \
  //     --key auth-token-x402 --value "$TOKEN" --if-match "$ETAG"
  //
  // `read -rs` keeps the password out of the shell's history, `printf '%s'`
  // takes a `%` in it as it is, and `tr` joins the lines GNU `base64` breaks
  // a long value into.
  const x402PortalKvs = config.enableX402PortalBasicAuth
    ? basicAuthKvs
    : undefined;
  const x402PortalFunction = new cloudfront.Function(
    scope,
    'X402PortalRoutingFunction',
    {
      functionName: `${config.envName}-soroban-explorer-x402-portal-routing`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      ...(x402PortalKvs && { keyValueStore: x402PortalKvs }),
      code: cloudfront.FunctionCode.fromInline(
        apiSpaRoutingFunctionCode({
          prefix: '/api/x402-facilitator',
          ...(x402PortalKvs && {
            auth: {
              kvsId: x402PortalKvs.keyValueStoreId,
              key: 'auth-token-x402',
              realm: 'x402 facilitator',
            },
          }),
          move: legacyMove,
        })
      ),
    }
  );

  // Every `/api/*` path no portal and not the hub takes (task 0670): the
  // Prices portal's routes from when it lived at `/api/` answer `301` to the
  // same path under `/api/prices-api`, the rest `302` to the hub: a path that
  // is unknown today can be the next portal's, and a browser keeps a `301`.
  const apiHubRedirectFunction = new cloudfront.Function(
    scope,
    'ApiHubRedirectFunction',
    {
      functionName: `${config.envName}-soroban-explorer-api-hub-redirect`,
      runtime: cloudfront.FunctionRuntime.JS_2_0,
      code: cloudfront.FunctionCode.fromInline(
        apiHubRedirectFunctionCode(portalMove, legacyMove)
      ),
    }
  );

  return {
    // Second, independently-built SPA (task 0519) — its own S3 origin,
    // gated independently of the main behaviors via `apiSpaFunction`
    // above, which also handles SPA routing. Single short-TTL behavior
    // for now; split out a long-TTL asset sub-path once this SPA's
    // build output layout is known.
    //
    // `/api/prices-api/*` requires the literal trailing slash, so it does
    // NOT match bare `/api/prices-api` — that needs its own exact-match
    // behavior, redirected to `/api/prices-api/` by the same function.
    // Behaviors match in this order: both stay above `/api/*`.
    '/api/prices-api': apiSpaBehavior(apiSpaFunction),
    '/api/prices-api/*': apiSpaBehavior(apiSpaFunction),
    // The x402 portal (task 0670): the same bucket, under the
    // `api/x402-facilitator/` prefix, and the same pair of behaviors, above
    // `/api/*` too.
    '/api/x402-facilitator': apiSpaBehavior(x402PortalFunction),
    '/api/x402-facilitator/*': apiSpaBehavior(x402PortalFunction),
    // The portal hub (task 0660), a route of the main SPA. Bare `/api`
    // already falls to the default behavior; `/api/` would match `/api/*`.
    '/api/': shared.explorerPage,
    // The redirects (tasks 0608, 0660, 0670): the function answers `301`
    // or `302` before the origin is reached; the origin is there because a
    // behavior needs one.
    ...(config.apiHub && {
      '/api/hub/*': apiHubBackendBehavior(scope, config, shared),
    }),
    '/api/*': apiSpaBehavior(apiHubRedirectFunction),
  };
}
