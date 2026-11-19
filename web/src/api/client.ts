import { client } from '@rumblefish/api-types';

import { apiBaseUrl, turnstileSiteKey } from './config.js';
import { createSession } from './session.js';

// `credentials: 'include'`: the free-tier session is an HttpOnly cookie on the
// API host (task 0677), and a cross-origin read carries it only with this.
client.setConfig({ baseUrl: apiBaseUrl, credentials: 'include' });

// Free-tier access layer (task 0277), wired only when the site key is set.
// A read goes out at once; a 401 means no valid session cookie, so the tab
// renews it and repeats the read once — the page never sees that 401.
if (turnstileSiteKey) {
  const session = createSession({ siteKey: turnstileSiteKey, apiBaseUrl });
  // How many mints had finished when each read went out (see `renew`).
  const mintsWhenSent = new WeakMap<Request, number>();

  client.interceptors.request.use((request) => {
    mintsWhenSent.set(request, session.mints());
    return request;
  });

  client.interceptors.response.use(async (response, request) => {
    if (response.status !== 401) return response;
    // A read the map does not know counts as sent now, so it gets a fresh
    // check rather than a bare repeat that would read as blocked cookies.
    const seenMints = mintsWhenSent.get(request) ?? session.mints();
    if (!(await session.renew(seenMints))) return response;
    // Reads are GETs without a body, so the same Request can go out again.
    const repeated = await fetch(request);
    if (repeated.status === 401) session.cookiesBlocked();
    return repeated;
  });
}

// Always present a real `Error` to downstream code so consumers get a usable
// `.message` and stack trace. Attach `.status` (from the response, since the
// raw ErrorEnvelope body of ADR 0008 carries `code/message` but not status)
// and preserve the original thrown body as `.body` for typed access.
client.interceptors.error.use((error, response) => {
  const status = response?.status;

  if (error instanceof Error) {
    return Object.assign(error, { status });
  }

  // F-AF-4: only adopt the envelope `message` when it is actually a string —
  // a non-string (object / array) would `String()`-coerce to "[object Object]"
  // and surface as the user-facing error text.
  const rawMessage =
    error && typeof error === 'object' && 'message' in error
      ? (error as { message: unknown }).message
      : null;
  const envelopeMessage = typeof rawMessage === 'string' ? rawMessage : null;
  const message =
    envelopeMessage ??
    (typeof error === 'string' && error.length > 0
      ? error
      : `Request failed (HTTP ${status ?? 'unknown'})`);

  return Object.assign(new Error(message), { status, body: error });
});
