/**
 * CloudFront Function source for the Cloudflare origin-secret lock
 * (task 0277 / ADR 0048, Decision 4a). Rejects any viewer request whose
 * `x-origin-secret` header does not match the expected value held in a
 * CloudFront KeyValueStore (key `origin-secret`).
 *
 * CloudFront cannot do viewer mTLS, so the `*.cloudfront.net` distribution
 * is locked to Cloudflare with a shared secret instead: Cloudflare injects
 * the header on every proxied request (a Transform Rule), and this function
 * rejects anything reaching the distribution directly without it.
 *
 * The expected secret is populated OUT-OF-BAND — never in this code, the
 * CloudFormation template, or git. Canonical source is AWS Secrets Manager
 * (`soroban/production/cloudflare/origin-secret`), alongside the mTLS
 * bundles; the KVS is a runtime copy and Cloudflare reads the same secret:
 *
 *   KVS_ARN=$(aws cloudfront list-key-value-stores \
 *     --query "KeyValueStoreList.Items[?Name=='production-soroban-explorer-origin-secret'].ARN" --output text)
 *   ETAG=$(aws cloudfront-keyvaluestore describe-key-value-store --kvs-arn "$KVS_ARN" --query "ETag" --output text)
 *   SECRET=$(aws secretsmanager get-secret-value \
 *     --secret-id soroban/production/cloudflare/origin-secret --query SecretString --output text)
 *   aws cloudfront-keyvaluestore put-key --kvs-arn "$KVS_ARN" \
 *     --key origin-secret --value "$SECRET" --if-match "$ETAG"
 *
 * ⚠ SECRET HYGIENE: `put-key --value "$SECRET"` passes the secret as a CLI
 * ARGUMENT — it lands in shell history and is visible via `ps auxww` to other
 * users while it runs, and WOULD be echoed into a CI job log. Run it only
 * interactively with history disabled (`set +o history` around the block, or
 * HISTCONTROL=ignorespace + a leading space). The CloudFront KVS API has no
 * stdin form for the value; if this ever moves to CI, mask it (and prefer
 * `--cli-input-json file://` from a /dev/shm temp file, shredded after).
 * `SECRET=$(...)` and `export CLOUDFLARE_API_TOKEN=$(...)` are safe (command
 * substitution keeps the value off argv).
 *
 * Runtime: cloudfront-js-2.0 — supports `import` and top-level `await`,
 * required for `cloudfront.kvs()` access.
 *
 * Closed-by-default: if the KVS lookup fails (typically because the key has
 * not been populated yet, e.g. immediately after first deploy) the function
 * returns 503 rather than allowing requests through.
 *
 * Rotation is dual-accept: during a Cloudflare secret flip, extend this to
 * compare against two KVS keys (old + new), then drop the old one once
 * Cloudflare only sends the new value. This single-key form is the steady
 * state. The secret is never logged.
 */
export function originSecretFunctionCode(kvsId: string): string {
  return `
import cf from 'cloudfront';
const kvs = cf.kvs('${kvsId}');

async function handler(event) {
  var request = event.request;
  var headers = request.headers;
  var expected;
  try {
    expected = await kvs.get('origin-secret');
  } catch (e) {
    return {
      statusCode: 503,
      statusDescription: 'Service Unavailable',
      headers: { 'cache-control': { value: 'no-store' } },
      body: 'Origin lock not configured'
    };
  }
  // Trim ONLY the expected (KVS) side — the canonical secret can pick up a
  // trailing newline when copied Secrets Manager -> KVS via shell. Do NOT trim
  // the attacker-controlled provided value. An empty/whitespace expected =
  // misconfigured -> fail closed (503), never an open distribution.
  var exp = (expected || '').trim();
  if (!exp) {
    return {
      statusCode: 503,
      statusDescription: 'Service Unavailable',
      headers: { 'cache-control': { value: 'no-store' } },
      body: 'Origin lock not configured'
    };
  }
  var provided = headers['x-origin-secret'];
  if (!provided || provided.value !== exp) {
    return {
      statusCode: 403,
      statusDescription: 'Forbidden',
      headers: { 'cache-control': { value: 'no-store' } },
      body: 'Forbidden'
    };
  }
  return request;
}
`.trim();
}
