/**
 * CloudFront Function source for the Prices portal's old homes (task 0660):
 * `/prices-api` + `/prices-api/*` (task 0608 until task 0660), `/pricing-api`
 * + `/pricing-api/*` (live for a few hours on 2026-10-02) and every `/api/*`
 * path the portal hub (`/api/`) and the portal itself (`/api/prices-api/*`)
 * do not take — the portal lived at `/api/` until task 0608. Every request
 * answers `301` to the same path under `/api/prices-api`, query string kept:
 * the Prices backend sends the OAuth popup back to `/prices-api/?signin=…`
 * until its own deploy, and a link shared before any of the moves keeps
 * working. One hop, never a chain. No auth check: the target is gated by its
 * own behavior.
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
        value: '/api/prices-api' +
          (request.uri.replace(/^\\/(prices-api|pricing-api|api)/, '') || '/') +
          (query.length ? '?' + query.join('&') : '')
      }
    }
  };
}
`.trim();
