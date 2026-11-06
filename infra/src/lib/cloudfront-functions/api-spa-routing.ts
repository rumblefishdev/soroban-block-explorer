import { basicAuthCheckSnippet } from './basic-auth.js';
import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * CloudFront Function source for the `/api/prices-api` and `/api/prices-api/*`
 * behaviors of the second SPA, the Stellar Prices portal (task 0519; at `/api`
 * until task 0608 and at `/prices-api` until task 0660, both now only redirect
 * — `api-path-redirect.ts`). Combines the concerns that must live in one
 * viewer-request function, since CloudFront allows only one per behavior:
 *
 * - The host move (task 0664), before anything else: the old host answers
 *   `301` to the same path on the new one, or, while the portal stays on the
 *   old host (`pricesPortalOnLegacyHost`), the new host answers `302` back to
 *   it. The bare prefix gets its trailing slash in the same hop.
 * - SPA routing, always active regardless of auth: any request whose last
 *   path segment has no `.` (i.e. not a real static asset) is rewritten to
 *   `/api/prices-api/index.html` — this covers the bucket-root case
 *   (`/api/prices-api/`) and deep-linked client-side routes
 *   (`/api/prices-api/anything/nested`) alike. Bare `/api/prices-api` (no
 *   trailing slash) doesn't match the `/api/prices-api/*` path pattern at all
 *   — CloudFront requires the literal trailing slash — so it's 301-redirected
 *   to `/api/prices-api/` here first, via a dedicated exact-match
 *   `/api/prices-api` behavior that also points at this function.
 * - Basic auth, only when `kvsId` is given (i.e. `config.enableApiSpaBasicAuth`
 *   is on) — reuses the exact check from `basic-auth.ts` against the same
 *   shared KeyValueStore, so there's one auth algorithm to keep in sync,
 *   not two.
 */
export function apiSpaRoutingFunctionCode(
  kvsId: string | undefined,
  move?: HostMove
): string {
  const authImport = kvsId
    ? `import cf from 'cloudfront';\nconst kvs = cf.kvs('${kvsId}');\n`
    : '';
  const authCheck = kvsId ? basicAuthCheckSnippet('API') : '';

  return `
${authImport}
${MOVED_JS}

async function handler(event) {
  var request = event.request;
  var uri = request.uri;
${legacyHostSnippet(move)}
  if (uri === '/api/prices-api') uri = '/api/prices-api/';
  if (origin || uri !== request.uri) return moved(origin + uri + search, status);
${authCheck}
  var lastSegment = uri.slice(uri.lastIndexOf('/') + 1);
  if (lastSegment.indexOf('.') === -1) {
    request.uri = '/api/prices-api/index.html';
  }

  return request;
}
`.trim();
}
