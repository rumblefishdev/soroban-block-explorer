/**
 * CloudFront Function source for the `/api` and `/api/*` behaviors: the
 * Prices portal's old home (task 0519), moved to `/pricing-api` by task 0608.
 * Every request answers `301` to the same path under `/pricing-api`, query
 * string kept — the Prices backend still sends the OAuth popup back to
 * `/api/?signin=…` until its own deploy, and a link shared before the move
 * keeps working. Bare `/api` goes straight to `/pricing-api/`, one hop rather
 * than two. No auth check: the target is gated by its own behavior.
 *
 * ponytail: query values are re-joined as the event hands them over, with no
 * re-encoding — exact for what the portal sends (`signin=failed`,
 * `issue=…`); if a percent-encoded value ever arrives decoded, wrap
 * `v.value` in `encodeURIComponent`.
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
        value: '/pricing-api' + (request.uri.slice(4) || '/') +
          (query.length ? '?' + query.join('&') : '')
      }
    }
  };
}
`.trim();
