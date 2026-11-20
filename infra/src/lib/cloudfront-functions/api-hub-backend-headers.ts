/**
 * CloudFront Function source for `/api/hub/*` (task 0680), a viewer-request
 * function: it deletes the viewer's own copies of the headers the backend
 * trusts CloudFront for, `X-Hub-Origin-Secret` and `X-Hub-Origin-Revision`
 * (origin custom headers) and `CloudFront-Viewer-Address` (the address the
 * hub's rate limit reads).
 *
 * AWS documents that an origin custom header overwrites a viewer's header of
 * the same name, but its pages say nothing of a viewer-supplied
 * `CloudFront-Viewer-Address`; so the function does not rely on either.
 * CloudFront adds its own values after the viewer-request stage.
 */
export function apiHubBackendHeadersFunctionCode(): string {
  return `
var TRUSTED = ['x-hub-origin-secret', 'x-hub-origin-revision', 'cloudfront-viewer-address'];

function handler(event) {
  var request = event.request;
  for (var i = 0; i < TRUSTED.length; i++) {
    delete request.headers[TRUSTED[i]];
  }
  return request;
}
`.trim();
}
