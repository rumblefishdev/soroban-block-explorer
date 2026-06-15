import { classifyError, isMissingResource } from '../classifyError.js';

import { NotFoundState, type NotFoundEntity } from './NotFoundState.js';
import { QueryErrorState } from './QueryErrorState.js';

interface DetailErrorStateProps {
  /** Raw query error for the detail resource. */
  error: unknown;
  /** Entity kind, used for the not-found copy. */
  entity: NotFoundEntity;
  /** The id from the URL, echoed in the not-found copy. */
  identifier?: string;
  /** Retry handler wired to the query's `refetch`. */
  onRetry?: () => void;
  /** Vertical padding passed to whichever state renders. */
  py?: number;
}

/**
 * Shared detail-page error switch. A missing resource — 404 or a malformed-id
 * 400/422 (see {@link isMissingResource}) — renders the entity-specific
 * NotFoundState; every other error is delegated to {@link QueryErrorState}, so
 * detail pages get the same rate-limit / transient / generic handling as the
 * lists. Used by all seven detail pages.
 */
export function DetailErrorState({
  error,
  entity,
  identifier,
  onRetry,
  py,
}: DetailErrorStateProps) {
  return isMissingResource(classifyError(error)) ? (
    <NotFoundState entity={entity} identifier={identifier} py={py} />
  ) : (
    <QueryErrorState error={error} onRetry={onRetry} py={py} />
  );
}
