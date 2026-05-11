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
