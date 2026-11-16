import { basicAuthCheckSnippet } from './basic-auth.js';
import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/** The Basic Auth gate of one portal: where its credentials are. */
export interface ApiSpaAuth {
  /** Id of the CloudFront KeyValueStore the function reads. */
  readonly kvsId: string;
  /** The key in it that holds `base64(user:password)`. */
  readonly key: string;
  /** The realm the browser's prompt names. */
  readonly realm: string;
}

export interface ApiSpaRouting {
  /** Where the portal is served, no trailing slash: `/api/prices-api`. */
  readonly prefix: string;
  /** Gate the portal behind Basic Auth; left out, it is public. */
  readonly auth?: ApiSpaAuth;
  readonly move?: HostMove;
}

/**
 * CloudFront Function source for the two behaviors of a portal served from
 * the API SPA bucket under `/api/`: `<prefix>` and `<prefix>/*`. One source,
 * one function per portal: the Stellar Prices portal at `/api/prices-api`
 * (task 0519; at `/api` until task 0608 and at `/prices-api` until task 0660,
 * both now only redirect — `api-path-redirect.ts`, `api-hub-redirect.ts`) and
 * the x402 facilitator's portal at `/api/x402-facilitator` (task 0670).
 * Combines the concerns that must live in one viewer-request function, since
 * CloudFront allows only one per behavior:
 *
 * - The host move (task 0664), before anything else: the old host answers
 *   `301` to the same path on the new one, or, while the Prices portal stays
 *   on the old host (`pricesPortalOnLegacyHost`), the new host answers `302`
 *   back to it. The bare prefix gets its trailing slash in the same hop.
 * - SPA routing, always active regardless of auth: any request whose last
 *   path segment has no `.` (i.e. not a real static asset) is rewritten to
 *   `<prefix>/index.html` — this covers the bucket-root case (`<prefix>/`)
 *   and deep-linked client-side routes (`<prefix>/anything/nested`) alike.
 *   Bare `<prefix>` (no trailing slash) doesn't match the `<prefix>/*` path
 *   pattern at all — CloudFront requires the literal trailing slash — so it's
 *   301-redirected to `<prefix>/` here first, via a dedicated exact-match
 *   `<prefix>` behavior that also points at this function.
 * - Basic auth, only when `auth` is given — reuses the exact check from
 *   `basic-auth.ts`, so there's one auth algorithm to keep in sync, not two.
 *   Closed by default: without `auth.key` in the store the answer is `503`.
 */
export function apiSpaRoutingFunctionCode({
  prefix,
  auth,
  move,
}: ApiSpaRouting): string {
  const authImport = auth
    ? `import cf from 'cloudfront';\nconst kvs = cf.kvs('${auth.kvsId}');\n`
    : '';
  const authCheck = auth ? basicAuthCheckSnippet(auth.realm, auth.key) : '';

  return `
${authImport}
${MOVED_JS}

async function handler(event) {
  var request = event.request;
  var uri = request.uri;
${legacyHostSnippet(move)}
  if (uri === '${prefix}') uri = '${prefix}/';
  if (origin || uri !== request.uri) return moved(origin + uri + search, status);
${authCheck}
  var lastSegment = uri.slice(uri.lastIndexOf('/') + 1);
  if (lastSegment.indexOf('.') === -1) {
    request.uri = '${prefix}/index.html';
  }

  return request;
}
`.trim();
}
