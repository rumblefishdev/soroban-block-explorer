/**
 * CloudWatch Synthetics canary handler for the origin-lockdown invariant
 * (task 0277 / ADR 0048, Step 7 acceptance criteria).
 *
 * Continuously asserts that the direct-origin bypass vectors stay BLOCKED —
 * i.e. answer with a client/server error (4xx/5xx), not a served 2xx
 * response. If any target answers 2xx (or a 3xx redirect) the run fails →
 * SuccessPercent drops → the alarm fires to the existing SNS→Slack topic.
 *
 * "Blocked" is asserted as `statusCode >= 400` rather than exactly 403 so the
 * canary does not false-alarm on the legitimate 503 the CloudFront Function
 * returns in the brief window after the lock is enabled but before the KVS is
 * populated. Any 2xx means the origin actually served content = a real bypass.
 *
 * Targets are supplied via environment variables (set by the CDK Canary only
 * for vectors whose lock is live, so nothing is hardcoded and an un-locked
 * origin is never probed):
 *   EXECUTE_API_URL  — raw API Gateway execute-api URL (403 once
 *                      disableExecuteApiEndpoint is set)
 *   CLOUDFRONT_URL   — *.cloudfront.net domain (403/503 once the
 *                      viewer-request Function rejects requests lacking
 *                      X-Origin-Secret)
 *
 * Plain `https` + throw-on-served — no Synthetics library dependency, so it is
 * runtime-version agnostic. A thrown error fails the canary run.
 *
 * Note: the API mTLS leg (direct REGIONAL custom-domain without a client
 * cert) fails at the TLS handshake, not with an HTTP status, so it is left to
 * the one-time negative-test matrix rather than this HTTP canary.
 */
export function originLockCanaryCode(): string {
  return `
const https = require('https');

// A locked origin must answer 4xx/5xx (403 once attached, 503 in the
// KVS-not-yet-populated window, etc.). Any 2xx/3xx means the request was
// served / not blocked -> the lockdown regressed.
function assertBlocked(url) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { timeout: 10000 }, (res) => {
      res.resume(); // drain
      const code = res.statusCode;
      if (code >= 400) {
        console.log('OK ' + url + ' -> ' + code + ' (blocked)');
        resolve();
      } else {
        reject(new Error(
          'ORIGIN LOCK REGRESSED: ' + url + ' answered ' + code +
          ' (expected 4xx/5xx)'
        ));
      }
    });
    req.on('timeout', () => req.destroy(new Error('timeout: ' + url)));
    req.on('error', reject);
  });
}

exports.handler = async function () {
  const targets = [process.env.EXECUTE_API_URL, process.env.CLOUDFRONT_URL]
    .filter(Boolean);
  if (targets.length === 0) {
    throw new Error('No origin-lock targets configured');
  }
  for (const url of targets) {
    await assertBlocked(url);
  }
  return 'origin lockdown holds';
};
`.trim();
}
