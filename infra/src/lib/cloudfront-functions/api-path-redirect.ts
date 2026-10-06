/**
 * CloudFront Function source for the Prices portal's old homes: `/api` +
 * `/api/*` (task 0519 until task 0608) and `/pricing-api` + `/pricing-api/*`
 * (live for a few hours on 2026-10-02, before the name settled on
 * `/prices-api`). Every request answers `301` to the same path under
 * `/prices-api`, query string kept — the Prices backend still sends the OAuth
 * popup back to `/api/?signin=…` until its own deploy, and a link shared
 * before either move keeps working. A bare old prefix goes straight to
 * `/prices-api/`, one hop rather than two. No auth check: the target is gated
 * by its own behavior.
 *
 * Query values are re-joined as the event hands them over. CloudFront hands
 * them over still percent-encoded (checked on production 2026-10-02:
 * `?x=a%26b&y=%C3%A9` came back as `a%26b` and `%C3%A9`), so that is exact.
 */
export const API_PATH_REDIRECT_FUNCTION_CODE = `
function handler(event) {
  var request = event.request;
  var query = [];
  Object.keys(request.querystring).forEach(function (name) {
    var param = request.querystring[name];
    (param.multiValue || [param]).forEach(function (v) {
      query.push(name + '=' + v.value);
    });
  });

  return {
    statusCode: 301,
    statusDescription: 'Moved Permanently',
    headers: {
      location: {
        value: '/prices-api' +
          (request.uri.replace(/^\\/(api|pricing-api)/, '') || '/') +
          (query.length ? '?' + query.join('&') : '')
      }
    }
  };
}
`.trim();
