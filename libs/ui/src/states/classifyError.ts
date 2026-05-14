export type ErrorKind =
  | 'not-found'
  | 'rate-limit'
  | 'transient'
  | 'validation'
  | 'unknown';

/**
 * Maps an error/response object to a coarse classification used by the
 * error-state components and by TanStack Query error handlers (task 0066).
 *
 * Accepts:
 *   - a `Response` (uses .status)
 *   - an object with a numeric `status` field (e.g. fetch errors, codegen client errors)
 *   - a native `Error` (network errors → transient)
 *   - anything else → 'unknown'
 */
export function classifyError(err: unknown): ErrorKind {
  if (err == null) return 'unknown';

  const status =
    typeof err === 'object' && 'status' in err
      ? (err as { status: unknown }).status
      : undefined;

  if (typeof status === 'number') {
    if (status === 404) return 'not-found';
    if (status === 429) return 'rate-limit';
    if (status >= 500 && status < 600) return 'transient';
    if (status === 400 || status === 422) return 'validation';
  }

  if (err instanceof TypeError) {
    return 'transient';
  }

  return 'unknown';
}
