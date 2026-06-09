import { client } from '@rumblefish/api-types';

import { apiBaseUrl } from './config.js';
import { ensureSessionToken, invalidateSession } from './session.js';

client.setConfig({ baseUrl: apiBaseUrl });

// Free-tier access layer (task 0277): attach the session JWT to every request.
// No-op until `VITE_TURNSTILE_SITE_KEY` is set — `ensureSessionToken()` returns
// `null` and the request goes out unchanged (the backend gate is also dark).
client.interceptors.request.use(async (request) => {
  const token = await ensureSessionToken();
  if (token) {
    request.headers.set('Authorization', `Bearer ${token}`);
  }
  return request;
});

// Always present a real `Error` to downstream code so consumers get a usable
// `.message` and stack trace. Attach `.status` (from the response, since the
// raw ErrorEnvelope body of ADR 0008 carries `code/message` but not status)
// and preserve the original thrown body as `.body` for typed access.
client.interceptors.error.use((error, response) => {
  const status = response?.status;

  // A 401 means the session JWT was rejected (expired/invalid) — drop it so the
  // next request re-solves Turnstile and mints a fresh one. Harmless when the
  // access layer is dark (no token was ever cached). Does not auto-retry the
  // failed request; TanStack Query's retry will, now with a valid session.
  if (status === 401) {
    invalidateSession();
  }

  if (error instanceof Error) {
    return Object.assign(error, { status });
  }

  const envelopeMessage =
    error && typeof error === 'object' && 'message' in error
      ? String((error as { message: unknown }).message)
      : null;
  const message =
    envelopeMessage ??
    (typeof error === 'string' && error.length > 0
      ? error
      : `Request failed (HTTP ${status ?? 'unknown'})`);

  return Object.assign(new Error(message), { status, body: error });
});
