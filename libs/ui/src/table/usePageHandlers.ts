import { useCallback } from 'react';

/**
 * Structural shape of the `page` field on a paginated API response.
 * Backend `PageInfo`: `has_more` flag plus optional opaque forward cursor.
 */
export interface PageInfoLike {
  has_more: boolean;
  cursor?: string | null;
}

export interface UsePageHandlersResult {
  /** `true` when the API reports more pages forward. */
  canNext: boolean;
  /** Invoke to advance via `goNext` to the next cursor (no-op if absent). */
  handleNext: () => void;
}

/**
 * Derives Next-button state + handler from a paginated response's
 * `page` field. Pairs with `useCursorPagination`'s `goNext` to remove
 * the repeated `nextCursor`/`canNext`/`handleNext` boilerplate every
 * paginated page would otherwise carry.
 */
export function usePageHandlers(
  page: PageInfoLike | undefined,
  goNext: (cursor: string) => void
): UsePageHandlersResult {
  const nextCursor = page?.has_more ? page.cursor ?? null : null;
  const canNext = nextCursor !== null;
  const handleNext = useCallback(() => {
    if (nextCursor) goNext(nextCursor);
  }, [nextCursor, goNext]);
  return { canNext, handleNext };
}
