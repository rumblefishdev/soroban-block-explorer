import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * The first segments under `/api/` that are pages of the API hub app (the
 * Prices portal's own pages included, `prices-api` being the old address of
 * its tab, and the x402 Facilitator's landing page, `x402-facilitator`, its
 * old address too). The list is closed: any other first segment is not
 * served.
 */
export const API_HUB_PAGES = [
  'login',
  'dashboard',
  'quick-start',
  'docs',
  'privacy-policy',
  'prices-api',
  'x402-facilitator',
] as const;

/**
 * CloudFront Function source for the API hub, one for `/api/*` and the two
 * `/api/prices-api` behaviors: a viewer-request function, in this order:
 *
 * - The host move (task 0664), `301` to the same address on the new host, in
 *   the same hop as every redirect below.
 * - Bare `/api`, bare `/api/prices-api` and bare `/api/x402-facilitator` get
 *   their trailing slash (`301`): CloudFront's `<prefix>/*` pattern does not
 *   match the first two, and the two landing pages keep one address each.
 * - Then by the first segment after `/api/`:
 *   - none (`/api/`), or one of `API_HUB_PAGES`: a last segment with a `.` is
 *     a file and goes to the bucket as asked; anything else is rewritten to
 *     `/api/index.html`, the hub's page, query string untouched.
 *   - `openapi.json`: `301` to `/api/prices-api/openapi.json`, query kept, as
 *     every link to the Prices portal's old root file has gone since task
 *     0608.
 *   - anything else: `302` to `/api/`, without its query string. A `302`: an
 *     address nobody serves today can be a page tomorrow.
 *
 * The match is exact and case-sensitive: `/api/LOGIN` and `/api/loginx` are
 * not pages.
 */
export function apiHubRoutingFunctionCode(move: HostMove | undefined): string {
  return `
${MOVED_JS}

var PAGES = ${JSON.stringify(API_HUB_PAGES)};

function handler(event) {
  var request = event.request;
  var uri = request.uri;
${legacyHostSnippet(move)}
  if (
    uri === '/api' ||
    uri === '/api/prices-api' ||
    uri === '/api/x402-facilitator'
  ) {
    return moved(origin + uri + '/' + search, status);
  }
  var rest = uri.slice('/api/'.length);
  var first = rest.split('/')[0];
  if (rest !== '' && PAGES.indexOf(first) === -1) {
    if (first === 'openapi.json') {
      return moved(origin + '/api/prices-api/' + rest + search, status);
    }
    return moved(origin + '/api/', 302);
  }
  if (origin) return moved(origin + uri + search, status);
  var lastSegment = uri.slice(uri.lastIndexOf('/') + 1);
  if (lastSegment.indexOf('.') === -1) {
    request.uri = '/api/index.html';
  }
  return request;
}
`.trim();
}
