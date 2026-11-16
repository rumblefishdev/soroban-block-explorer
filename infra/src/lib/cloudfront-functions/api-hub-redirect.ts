import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * The Prices portal's routes and root files from when it lived at `/api/`
 * (until task 0608): its router's paths (`web/portal/src/app/app.tsx` in
 * stellar-prices-api, the same six before and after the move, less `/`) and
 * the one file of its `public/` a link points at. The list is closed: the
 * portal no longer lives here, so a route it gains later was never under
 * `/api/`.
 */
export const PRICES_PORTAL_LEGACY_ROUTES = [
  'docs',
  'dashboard',
  'login',
  'quick-start',
  'privacy-policy',
  'openapi.json',
] as const;

/**
 * CloudFront Function source for `/api/*`: every path under `/api/` that no
 * portal (`/api/prices-api/*`, `/api/x402-facilitator/*`) and not the hub
 * (`/api/`) takes (task 0670).
 *
 * - A path whose first segment is one of `PRICES_PORTAL_LEGACY_ROUTES`
 *   answers `301` to the same path under `/api/prices-api`, query string
 *   kept, as every `/api/*` path did since task 0608: a link to the portal
 *   shared before it moved keeps working.
 * - Anything else answers `302` to the hub at `/api/`, without its query
 *   string. Before task 0670 it went to `/api/prices-api/…`, which read a
 *   mistyped or not-yet-served portal address as a Prices one. A `302`, not a
 *   `301`: a browser keeps a `301` for good, and a path that is unknown today
 *   can be the next portal's (`/api/x402-facilitator` was one until task
 *   0670).
 *
 * One hop, never a chain (task 0664): `portalMove` is the host move of the
 * Prices portal, `hubMove` the one of the main SPA, which serves the hub. They
 * differ while the portal stays on the old host (`pricesPortalOnLegacyHost`).
 */
export function apiHubRedirectFunctionCode(
  portalMove: HostMove | undefined,
  hubMove: HostMove | undefined
): string {
  const hubOrigin = hubMove
    ? `request.headers &&
    request.headers.host &&
    request.headers.host.value.toLowerCase() === '${hubMove.from}'
      ? 'https://${hubMove.to}'
      : ''`
    : `''`;

  return `
${MOVED_JS}

var PORTAL_ROUTES = ${JSON.stringify(PRICES_PORTAL_LEGACY_ROUTES)};

function handler(event) {
  var request = event.request;
${legacyHostSnippet(portalMove)}
  var rest = request.uri.slice('/api'.length);
  if (PORTAL_ROUTES.indexOf(rest.split('/')[1]) !== -1) {
    return moved(origin + '/api/prices-api' + rest + search, status);
  }
  var hubOrigin = ${hubOrigin};
  return moved(hubOrigin + '/api/', 302);
}
`.trim();
}
