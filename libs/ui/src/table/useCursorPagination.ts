import { useCallback, useEffect, useRef } from 'react';

import {
  useTableUrlState,
  type UseTableUrlStateOptions,
  type UseTableUrlStateResult,
} from './useTableUrlState.js';

/**
 * Scope identifier extension over `UseTableUrlStateOptions`. When
 * `resetKey` flips (compared with `Object.is`), the URL cursor is reset.
 * Use on detail-page sections where the cursor belongs to a parent
 * entity (`resetKey: poolId`, `resetKey: contractId`).
 *
 * Initial mount does NOT reset — a pasted deep link `?cursor=...`
 * survives the first render.
 */
type Options = UseTableUrlStateOptions & { resetKey?: unknown };

export interface UseCursorPaginationResult
  extends Omit<UseTableUrlStateResult, 'setFilter'> {
  /** Current cursor (mirrors `state.cursor`). */
  cursor: string | null;
  /**
   * Navigate forward to `nextCursor`. Caller passes the cursor read
   * from the most recent response's `page.next_cursor`.
   */
  goNext: (nextCursor: string) => void;
  /**
   * Navigate backward to `prevCursor`. Caller passes the cursor read
   * from the most recent response's `page.prev_cursor`, or `null` to
   * jump to the first page (clears the URL `?cursor=`).
   *
   * Symmetric with `goNext` — `prev_cursor` is now provided directly
   * by the backend (no client-side stack required). See ADR for cursor
   * direction encoding.
   */
  goPrev: (prevCursor: string | null) => void;
  /**
   * Update a filter param. Drops the URL cursor (delegated to
   * `useTableUrlState.setFilter`) because cursors are filter-scoped on
   * the server.
   */
  setFilter: (key: string, value: string | null) => void;
  /**
   * Drop every filter key + the cursor in one URL update. Use for
   * "Clear filters" instead of multiple `setFilter(key, null)` calls
   * (which clobber each other — see `useTableUrlState.clearFilters`).
   * `sort`/`dir` are not filter keys, so they survive.
   */
  clearFilters: () => void;
  /** Clear the URL cursor. */
  reset: () => void;
}

/**
 * URL-as-state cursor pagination. Cursor lives in `?cursor=` query
 * param; `goNext` / `goPrev` rewrite the URL via `useTableUrlState`.
 *
 * Backend emits both forward (`next_cursor`) and backward
 * (`prev_cursor`) tokens in every `PageInfo`. The hook is a thin
 * URL-binding around those — it does not maintain any client-side
 * cursor history. A pasted deep link `?cursor=X` works identically
 * regardless of how the user landed there: the backend response
 * carries `prev_cursor`, which powers the Prev button without any
 * in-session walk requirement.
 *
 * Pairs with `useQuery` (not `useInfiniteQuery`): every distinct cursor
 * produces a distinct React Query `queryKey`, so revisiting a cursor
 * is a cache hit and renders instantly.
 *
 * @param options Forwarded to `useTableUrlState` — pass `filterKeys`
 *                here for pages that have filters.
 */
export function useCursorPagination(
  options?: Options
): UseCursorPaginationResult {
  const urlState = useTableUrlState(options);
  const {
    state,
    setCursor,
    setFilter: setFilterRaw,
    setFilters,
    clearFilters,
    resetCursor,
  } = urlState;

  const reset = useCallback(() => {
    resetCursor();
  }, [resetCursor]);

  // `resetKey` fires reset when the caller's scope identifier flips
  // (e.g. parent entity id changed). The `useRef` skips the initial
  // mount so a deep link with a pasted `?cursor=` survives — the cursor
  // only drops on a *change*, not on first render.
  const resetKey = options?.resetKey;
  const prevResetKey = useRef(resetKey);
  useEffect(() => {
    if (!Object.is(prevResetKey.current, resetKey)) {
      prevResetKey.current = resetKey;
      reset();
    }
  }, [resetKey, reset]);

  const goNext = useCallback(
    (nextCursor: string) => {
      setCursor(nextCursor);
    },
    [setCursor]
  );

  const goPrev = useCallback(
    (prevCursor: string | null) => {
      setCursor(prevCursor);
    },
    [setCursor]
  );

  const setFilter = useCallback(
    (key: string, value: string | null) => {
      setFilterRaw(key, value);
    },
    [setFilterRaw]
  );

  return {
    state,
    setCursor,
    setSort: urlState.setSort,
    setFilter,
    setFilters,
    clearFilters,
    resetCursor,
    cursor: state.cursor,
    goNext,
    goPrev,
    reset,
  };
}
