import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * CloudFront Function source for the Prices portal's old homes (task 0660):
 * `/prices-api` + `/prices-api/*` (task 0608 until task 0660), `/pricing-api`
 * + `/pricing-api/*` (live for a few hours on 2026-10-02) and every `/api/*`
 * path the portal hub (`/api/`) and the portal itself (`/api/prices-api/*`)
 * do not take — the portal lived at `/api/` until task 0608. Every request
 * answers `301` to the same path under `/api/prices-api`, query string kept,
 * so a link shared before any of the moves keeps working. One hop, never a
 * chain: when `move` takes the host elsewhere (task 0664), the `Location` is
 * already on the right one. No auth check: the target is gated by its own
 * behavior.
 */
export function apiPathRedirectFunctionCode(
  move: HostMove | undefined
): string {
  return `
${MOVED_JS}

function handler(event) {
  var request = event.request;
${legacyHostSnippet(move)}
  return moved(
    origin +
      '/api/prices-api' +
      (request.uri.replace(/^\\/(prices-api|pricing-api|api)/, '') || '/') +
      search,
    status
  );
}
`.trim();
}
