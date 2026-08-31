import { basicAuthCheckSnippet } from './basic-auth.js';

/**
 * CloudFront Function source for the `/api` and `/api/*` behaviors of the
 * second SPA (task 0519 follow-up). Combines two concerns that must live in
 * one viewer-request function, since CloudFront allows only one per
 * behavior:
 *
 * - SPA routing, always active regardless of auth: any request whose last
 *   path segment has no `.` (i.e. not a real static asset) is rewritten to
 *   `/api/index.html` — this covers the bucket-root case (`/api/`) and
 *   deep-linked client-side routes (`/api/anything/nested`) alike. Bare
 *   `/api` (no trailing slash) doesn't match the `/api/*` path pattern at
 *   all — CloudFront requires the literal trailing slash — so it's
 *   301-redirected to `/api/` here first, via a dedicated exact-match
 *   `/api` behavior that also points at this function.
 * - Basic auth, only when `kvsId` is given (i.e. `config.enableApiSpaBasicAuth`
 *   is on) — reuses the exact check from `basic-auth.ts` against the same
 *   shared KeyValueStore, so there's one auth algorithm to keep in sync,
 *   not two.
 */
export function apiSpaRoutingFunctionCode(kvsId: string | undefined): string {
  const authImport = kvsId
    ? `import cf from 'cloudfront';\nconst kvs = cf.kvs('${kvsId}');\n`
    : '';
  const authCheck = kvsId ? basicAuthCheckSnippet('API') : '';

  return `
${authImport}
async function handler(event) {
  var request = event.request;
  var uri = request.uri;

  if (uri === '/api') {
    return {
      statusCode: 301,
      statusDescription: 'Moved Permanently',
      headers: { location: { value: '/api/' } }
    };
  }
${authCheck}
  var lastSegment = uri.slice(uri.lastIndexOf('/') + 1);
  if (lastSegment.indexOf('.') === -1) {
    request.uri = '/api/index.html';
  }

  return request;
}
`.trim();
}
