import { classifyError } from '../classifyError.js';

import { GenericErrorState } from './GenericErrorState.js';
import { RateLimitState } from './RateLimitState.js';
import { TransientErrorState } from './TransientErrorState.js';

interface QueryErrorStateProps {
  /** Raw query error; classified into rate-limit / transient / generic. */
  error: unknown;
  /** Retry handler wired to the query's `refetch`. */
  onRetry?: () => void;
  /** Vertical padding passed through to the underlying EmptyState. */
  py?: number;
}

/**
 * Shared list/section error switch. Classifies a TanStack Query error and
 * renders the matching error-state — the rate-limit / transient / generic
 * three-way that was copy-pasted verbatim across ~14 list and section
 * components (SM-1). Detail pages that need not-found-vs-generic use
 * {@link isMissingResource} at their boundary instead (see DetailErrorState).
 */
export function QueryErrorState({ error, onRetry, py }: QueryErrorStateProps) {
  const kind = classifyError(error);
  if (kind === 'rate-limit')
    return <RateLimitState onRetry={onRetry} py={py} />;
  if (kind === 'transient')
    return <TransientErrorState onRetry={onRetry} py={py} />;
  return <GenericErrorState onRetry={onRetry} py={py} />;
}
