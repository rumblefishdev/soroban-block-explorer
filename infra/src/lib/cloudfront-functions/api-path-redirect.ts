import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * CloudFront Function source for the Prices portal's old homes outside
 * `/api/` (task 0660): `/prices-api` + `/prices-api/*` (task 0608 until task
 * 0660) and `/pricing-api` + `/pricing-api/*` (live for a few hours on
 * 2026-10-02). Every request answers `301` to the same path under
 * `/api/prices-api`, query string kept, so a link shared before any of the
 * moves keeps working. One hop, never a chain: when `move` takes the host
 * elsewhere (task 0664), the `Location` is already on the right one. No auth
 * check: the target is gated by its own behavior.
 *
 * The portal's oldest home, `/api/*`, has its own function since task 0670
 * (`api-hub-redirect.ts`), but the pattern below still strips `/api`. A deploy
 * updates this function in place, minutes before the distribution stops
 * sending `/api/*` to it; without `api` in the pattern, `/api/docs` would
 * answer `301` to `/api/prices-api/api/docs` in that window, and a browser
 * keeps a `301`. Drop it in a deploy of its own, once task 0670's is out.
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
