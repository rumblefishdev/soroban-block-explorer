import { useCallback } from 'react';

/**
 * Structural shape of the `page` field on a paginated API response.
 * Backend `PageInfo`: optional forward (`cursor`) and backward
 * (`prev_cursor`) opaque cursors. Presence of either string carries
 * the entire "is there another page" signal — `cursor !== null` means
 * a forward continuation exists, `prev_cursor !== null` a backward one.
 */
export interface PageInfoLike {
  cursor?: string | null;
  prev_cursor?: string | null;
}

export interface UsePageHandlersResult {
  /** `true` when the response carries a `prev_cursor`. */
  canPrev: boolean;
  /** `true` when the API reports more pages forward. */
  canNext: boolean;
  /**
   * Invoke to navigate backward via `goPrev` to the response's
   * `prev_cursor` (no-op when absent — the button is also disabled in
   * that case via `canPrev`).
   */
  handlePrev: () => void;
  /** Invoke to advance via `goNext` to the next cursor (no-op if absent). */
  handleNext: () => void;
}

/**
 * Derives symmetric Prev/Next button state + handlers from a paginated
 * response's `page` field. Pairs with `useCursorPagination`'s `goPrev`
 * / `goNext` to remove repeated cursor extraction boilerplate.
 *
 * Both directions are driven by the backend: `cursor` advances, and
 * `prev_cursor` goes back. The backend computes `prev_cursor` from the
 * direction-aware envelope (see ADR for cursor direction encoding), so
 * deep-link + Prev works on every response, with no client-side cursor
 * stack required.
 */
export function usePageHandlers(
  page: PageInfoLike | undefined,
  goNext: (cursor: string) => void,
  goPrev: (cursor: string | null) => void
): UsePageHandlersResult {
  const nextCursor = page?.cursor ?? null;
  const prevCursor = page?.prev_cursor ?? null;
  const canNext = nextCursor !== null;
  const canPrev = prevCursor !== null;
  const handleNext = useCallback(() => {
    if (nextCursor) goNext(nextCursor);
  }, [nextCursor, goNext]);
  const handlePrev = useCallback(() => {
    if (prevCursor) goPrev(prevCursor);
  }, [prevCursor, goPrev]);
  return { canPrev, canNext, handlePrev, handleNext };
}
