import { client } from '@rumblefish/api-types';

import { apiBaseUrl } from './config.js';

client.setConfig({ baseUrl: apiBaseUrl });

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
