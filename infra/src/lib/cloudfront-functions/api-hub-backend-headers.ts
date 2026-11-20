/**
 * CloudFront Function source for `/api/hub/*` (task 0680), a viewer-request
 * function:
 *
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
export function apiHubBackendHeadersFunctionCode(): string {
  return `
function handler(event) {
  var request = event.request;
  delete request.headers['x-hub-origin-secret'];
  delete request.headers['x-hub-origin-revision'];
  request.headers['x-hub-viewer-ip'] = { value: event.viewer.ip };
  return request;
}
`.trim();
}
