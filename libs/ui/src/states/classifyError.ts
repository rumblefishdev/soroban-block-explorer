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

/**
 * From the user's perspective, "no record under id `X`" and "id `X` is
 * malformed" are the same outcome: the thing they navigated to isn't
 * here. Detail-page routes carry the identifier in the URL, so a 400
 * INVALID_<id> (e.g. i64-overflow on `/ledgers/99999999999`) and a 404
 * NOT_FOUND both warrant the same entity-specific `NotFoundState` —
 * not the generic "Something went wrong" banner. Use this predicate to
 * unify the two cases at the detail-page error boundary (task 0251 H8).
 */
export function isMissingResource(kind: ErrorKind): boolean {
  return kind === 'not-found' || kind === 'validation';
}
