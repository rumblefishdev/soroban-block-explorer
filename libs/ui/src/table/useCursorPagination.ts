import { useCallback, useEffect, useRef, useState } from 'react';

import {
  useTableUrlState,
  type UseTableUrlStateOptions,
  type UseTableUrlStateResult,
} from './useTableUrlState.js';

/**
 * Scope identifier extension over `UseTableUrlStateOptions`. When
 * `resetKey` flips (compared with `Object.is`), the URL cursor +
 * prev-stack are reset. Use on detail-page sections where the cursor
 * belongs to a parent entity (`resetKey: poolId`, `resetKey: contractId`).
 *
 * Initial mount does NOT reset — a pasted deep link `?cursor=...`
 * survives the first render.
 */
type Options = UseTableUrlStateOptions & { resetKey?: unknown };

/**
 * Marker pushed onto the prev-cursor stack for the "no cursor" page
 * (page 0 / first page). Reserved sentinel — empty string can never be
 * a real cursor value (server contract: opaque non-empty token).
 */
const FIRST_PAGE = '';

/**
 * Upper bound on the prev-stack depth. Deep manual paging shouldn't
 * accumulate unbounded history in a single session; capping at 50
 * keeps Prev useful for the recent walk without leaking memory.
 */
const MAX_HISTORY = 50;

export interface UseCursorPaginationResult
  extends Omit<UseTableUrlStateResult, 'setFilter'> {
  /** Current cursor (mirrors `state.cursor`). */
  cursor: string | null;
  /** `true` when there is a remembered previous page on the stack. */
  canPrev: boolean;
  /**
   * Navigate to `nextCursor`. Pushes the current cursor onto the
   * prev-stack so `goPrev` can return here.
   */
  goNext: (nextCursor: string) => void;
  /**
   * Pop one entry off the prev-stack and navigate the URL to that
   * cursor. No-op when the stack is empty.
   */
  goPrev: () => void;
  /**
   * Update a filter param. Drops the URL cursor (delegated to
   * `useTableUrlState.setFilter`) AND clears the prev-stack — cursors
   * are filter-scoped on the server, so stale stack entries are
   * meaningless after a filter change.
   */
  setFilter: (key: string, value: string | null) => void;
  /** Clear cursor + stack. */
  reset: () => void;
}

/**
 * URL-as-state cursor pagination. Cursor lives in `?cursor=` query
 * param; `goNext` / `goPrev` rewrite the URL via `useTableUrlState`.
 *
 * Why a client-side prev stack: the API returns only a forward
 * (`next`) cursor per `PageInfo`, so the only way to step backwards
 * is to remember where we came from. Stack is kept in component
 * state — it survives Next/Prev clicks on the same mount but
 * resets on remount / refresh / filter change. That mirrors what
 * users expect: refresh keeps you at the current page (because the
 * cursor is in the URL), but you can only step back as far as you
 * walked forward this session.
 *
 * Pairs with `useQuery` (not `useInfiniteQuery`): every distinct
 * cursor produces a distinct React Query `queryKey`, so revisiting
 * a cursor is a cache hit and renders instantly.
 *
 * @param options Forwarded to `useTableUrlState` — pass
 *                `filterKeys` here for pages that have filters.
 */
export function useCursorPagination(
  options?: Options
): UseCursorPaginationResult {
  const urlState = useTableUrlState(options);
  const { state, setCursor, setFilter: setFilterRaw, resetCursor } = urlState;
  const [stack, setStack] = useState<string[]>([]);

  const reset = useCallback(() => {
    setStack([]);
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
      setStack((s) => {
        const next = [...s, state.cursor ?? FIRST_PAGE];
        return next.length > MAX_HISTORY ? next.slice(-MAX_HISTORY) : next;
      });
      setCursor(nextCursor);
    },
    [state.cursor, setCursor]
  );

  const goPrev = useCallback(() => {
    setStack((s) => {
      if (s.length === 0) return s;
      const prev = s[s.length - 1];
      setCursor(prev === FIRST_PAGE ? null : prev);
      return s.slice(0, -1);
    });
  }, [setCursor]);

  const setFilter = useCallback(
    (key: string, value: string | null) => {
      setStack([]);
      setFilterRaw(key, value);
    },
    [setFilterRaw]
  );

  return {
    state,
    setCursor,
    setSort: urlState.setSort,
    setFilter,
    resetCursor,
    cursor: state.cursor,
    canPrev: stack.length > 0,
    goNext,
    goPrev,
    reset,
  };
}
