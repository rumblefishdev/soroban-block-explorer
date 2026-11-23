import {
  type HostMove,
  legacyHostSnippet,
  MOVED_JS,
} from './legacy-host-redirect.js';

/**
 * CloudFront Function source for `/api/hub/*` (task 0680), a viewer-request
 * function:
 *
 * - The old host's request answers `301` to the same address on the new host,
 *   as every function on the distribution does first (task 0664).
 * - It writes the client's address into `X-Hub-Viewer-Ip` from
 *   `event.viewer.ip`, replacing any the viewer sent, as AWS's own example
 *   function writes `True-Client-IP`. The hub's rate limit reads it. An IPv6
 *   address is written as it is.
 * - It deletes the viewer's own `X-Hub-Origin-Secret` and
 *   `X-Hub-Origin-Revision`. AWS says an origin custom header replaces a
 *   viewer's of the same name; this does not rely on it.
 *
 * `CloudFront-Viewer-Address` is not touched: the guide does not say what
 * CloudFront does with a viewer-sent one, so nothing reads it.
 */
export function apiHubBackendHeadersFunctionCode(
  move: HostMove | undefined
): string {
  return `
${MOVED_JS}

function handler(event) {
  var request = event.request;
${legacyHostSnippet(move)}
  if (origin) return moved(origin + request.uri + search, status);
  delete request.headers['x-hub-origin-secret'];
  delete request.headers['x-hub-origin-revision'];
  request.headers['x-hub-viewer-ip'] = { value: event.viewer.ip };
  return request;
}
`.trim();
}
