/**
 * The SPA's old host only redirects (task 0664): every request to
 * `legacyDomainName` answers `301` to `https://<domainName>`, path and query
 * kept. CloudFront allows one viewer-request function per behavior, so every
 * function on the distribution starts with `legacyHostSnippet`; the behaviors
 * that had none get `legacyHostRedirectFunctionCode`. A function that rewrites
 * the path itself (the portal's old homes) prefixes its own `Location` with
 * `origin`, so an old link takes one hop, never two.
 *
 * The Prices portal can go the other way for a while (`pricesPortalOnLegacyHost`):
 * its functions take a `temporary` move from the new host back to the old one,
 * answered with `302` so no browser keeps it.
 */
export interface HostMove {
  /** The host requests leave, e.g. `sorobanscan.rumblefish.dev`. */
  readonly from: string;
  /** The host they go to, e.g. `sorobanscan.com`. */
  readonly to: string;
  /** `302` instead of `301`: the move will be undone. */
  readonly temporary?: boolean;
}

/** `moved(location, status)`: a `301`, or a `302` when `status` says so. */
export const MOVED_JS = `
function moved(location, status) {
  return {
    statusCode: status === 302 ? 302 : 301,
    statusDescription: status === 302 ? 'Found' : 'Moved Permanently',
    headers: { location: { value: location } }
  };
}
`.trim();

/**
 * Handler-body JS that defines `search` (the query string, `?…` or empty),
 * `origin` (`https://<to>` on the `from` host, else empty) and `status` (`302`
 * for a temporary move of the host, else `301`). Needs `request`.
 *
 * Query values are re-joined as the event hands them over. CloudFront hands
 * them over still percent-encoded (checked on production 2026-10-02:
 * `?x=a%26b&y=%C3%A9` came back as `a%26b` and `%C3%A9`), so that is exact.
 */
export function legacyHostSnippet(move: HostMove | undefined): string {
  const origin = move
    ? `var host = request.headers && request.headers.host
    ? request.headers.host.value.toLowerCase()
    : '';
  var origin = host === '${move.from}' ? 'https://${move.to}' : '';
  var status = origin ? ${move.temporary ? 302 : 301} : 301;`
    : `var origin = '';
  var status = 301;`;

  return `
  var query = [];
  Object.keys(request.querystring || {}).forEach(function (name) {
    var param = request.querystring[name];
    (param.multiValue || [param]).forEach(function (v) {
      query.push(name + '=' + v.value);
    });
  });
  var search = query.length ? '?' + query.join('&') : '';
  ${origin}
`;
}

/** The function for behaviors that have no other viewer-request function. */
export function legacyHostRedirectFunctionCode(move: HostMove): string {
  return `
${MOVED_JS}

function handler(event) {
  var request = event.request;
${legacyHostSnippet(move)}
  if (origin) return moved(origin + request.uri + search, status);
  return request;
}
`.trim();
}
