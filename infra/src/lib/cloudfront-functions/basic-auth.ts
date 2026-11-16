/**
 * CloudFront Function source for HTTP Basic Auth gating, backed by a
 * CloudFront KeyValueStore. The KVS holds one key per gate, each a
 * pre-encoded `base64(user:password)` value: `auth-token` for the main site
 * and the Prices portal, `auth-token-x402` for the x402 portal (task 0670), so
 * each password changes without the other. Credentials
 * are populated out-of-band (`aws cloudfront-keyvaluestore put-key`)
 * and never live in this code or in the CDK template.
 *
 * Runtime: cloudfront-js-2.0 — supports `import` and top-level `await`,
 * required for `cloudfront.kvs()` access.
 *
 * Closed-by-default: if the KVS lookup fails (typically because the key
 * has not been populated yet, e.g. immediately after first deploy), the
 * function returns 503 rather than allowing requests through.
 */
/**
 * The KVS-lookup-and-compare check itself, factored out so other CloudFront
 * Function sources (e.g. `api-spa-routing.ts`, task 0519's follow-up) can
 * embed the same auth check without duplicating the KVS error handling.
 * Assumes the enclosing function already has `const kvs = cf.kvs(...)` in
 * scope and is `async`. Returns a statement block, not a full function body.
 * `key` is the KVS key that holds this gate's credentials.
 */
export function basicAuthCheckSnippet(
  realm: string,
  key = 'auth-token'
): string {
  return `
  var headers = request.headers;
  var expected;
  try {
    var token = await kvs.get('${key}');
    expected = 'Basic ' + token;
  } catch (e) {
    return {
      statusCode: 503,
      statusDescription: 'Service Unavailable',
      headers: { 'cache-control': { value: 'no-store' } },
      body: 'Auth not configured'
    };
  }
  if (!headers.authorization || headers.authorization.value !== expected) {
    return {
      statusCode: 401,
      statusDescription: 'Unauthorized',
      headers: { 'www-authenticate': { value: 'Basic realm="${realm}"' } }
    };
  }
`;
}

export function basicAuthFunctionCode(kvsId: string): string {
  return `
import cf from 'cloudfront';
const kvs = cf.kvs('${kvsId}');

async function handler(event) {
  var request = event.request;
${basicAuthCheckSnippet('Staging')}
  return request;
}
`.trim();
}
