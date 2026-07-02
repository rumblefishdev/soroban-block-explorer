import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import type { SortDirection } from './ExplorerTable.js';

const DEFAULT_cursorParam = 'cursor';
const SORT_PARAM = 'sort';
const DIR_PARAM = 'dir';

export interface TableUrlState {
  cursor: string | null;
  sortBy: string | null;
  sortDir: SortDirection;
  filters: Record<string, string>;
}

export interface UseTableUrlStateOptions {
  defaultSortBy?: string;
  defaultSortDir?: SortDirection;
  filterKeys?: readonly string[];
  /**
   * Query-param key for the cursor. Defaults to `'cursor'`. Override
   * when multiple paginated sections coexist on the same route so each
   * section gets its own URL key (e.g. `'cursor_p'`, `'cursor_t'` on
   * the LP detail page) instead of clobbering each other.
   */
  cursorParam?: string;
}

export interface UseTableUrlStateResult {
  state: TableUrlState;
  setCursor: (cursor: string | null) => void;
  setSort: (sortBy: string, sortDir: SortDirection) => void;
  setFilter: (key: string, value: string | null) => void;
  /**
   * Drop every `filterKeys` entry (and the cursor) in a SINGLE URL
   * update. The `sort`/`dir` params are NOT filter keys, so they survive
   * — which is what "Clear filters" wants (sort does not narrow results).
   *
   * Must be one update: calling `setFilter(key, null)` N times in a row
   * does NOT work — react-router's functional `setSearchParams` hands each
   * synchronous call the same pre-render params, so the last call clobbers
   * the earlier ones and only one key actually clears.
   */
  clearFilters: () => void;
  /**
   * Set or delete SEVERAL filter keys (a `null` value deletes) plus drop the
   * cursor in ONE URL update. Use when a single user action changes more than
   * one filter at once — separate `setFilter` calls clobber each other, for the
   * same reason `clearFilters` exists (see the note above).
   */
  setFilters: (updates: Record<string, string | null>) => void;
  resetCursor: () => void;
}

export function useTableUrlState(
  options: UseTableUrlStateOptions = {}
): UseTableUrlStateResult {
  const {
    defaultSortBy,
    defaultSortDir = 'desc',
    filterKeys = [],
    cursorParam = DEFAULT_cursorParam,
  } = options;
  const [params, setParams] = useSearchParams();

  // Callers commonly pass `filterKeys` inline (`{ filterKeys: ['q', 'op'] }`),
  // which yields a new array reference every render. Depending on that array
  // directly would invalidate the memo every render and re-issue a new
  // `state` / `state.filters` identity to every downstream effect/memo.
  // Collapse the array into a stable string key (with `|` separator so
  // distinct key lists do not collide) and reparse inside the memo — that
  // way the memo's only array-shaped dep is the string itself, and the
  // closure captures nothing whose identity changes per render.
  const filterKeysKey = filterKeys.join('|');

  const state = useMemo<TableUrlState>(() => {
    const filters: Record<string, string> = {};
    const keys = filterKeysKey ? filterKeysKey.split('|') : [];
    for (const key of keys) {
      const v = params.get(key);
      if (v) filters[key] = v;
    }
    const rawDir = params.get(DIR_PARAM);
    const sortDir: SortDirection =
      rawDir === 'asc' || rawDir === 'desc' ? rawDir : defaultSortDir;
    return {
      cursor: params.get(cursorParam),
      sortBy: params.get(SORT_PARAM) ?? defaultSortBy ?? null,
      sortDir,
      filters,
    };
  }, [params, filterKeysKey, defaultSortBy, defaultSortDir, cursorParam]);

  const update = useCallback(
    (mutator: (next: URLSearchParams) => void) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          mutator(next);
          return next;
        },
        { replace: true }
      );
    },
    [setParams]
  );

  const setCursor = useCallback(
    (cursor: string | null) => {
      update((next) => {
        if (cursor) next.set(cursorParam, cursor);
        else next.delete(cursorParam);
      });
    },
    [update, cursorParam]
  );

  const setSort = useCallback(
    (sortBy: string, sortDir: SortDirection) => {
      update((next) => {
        next.set(SORT_PARAM, sortBy);
        next.set(DIR_PARAM, sortDir);
        next.delete(cursorParam);
      });
    },
    [update, cursorParam]
  );

  const setFilter = useCallback(
    (key: string, value: string | null) => {
      update((next) => {
        if (value) next.set(key, value);
        else next.delete(key);
        next.delete(cursorParam);
      });
    },
    [update, cursorParam]
  );

  const setFilters = useCallback(
    (updates: Record<string, string | null>) => {
      update((next) => {
        for (const [key, value] of Object.entries(updates)) {
          if (value) next.set(key, value);
          else next.delete(key);
        }
        next.delete(cursorParam);
      });
    },
    [update, cursorParam]
  );

  const clearFilters = useCallback(() => {
    update((next) => {
      const keys = filterKeysKey ? filterKeysKey.split('|') : [];
      for (const key of keys) next.delete(key);
      next.delete(cursorParam);
    });
  }, [update, filterKeysKey, cursorParam]);

  const resetCursor = useCallback(() => setCursor(null), [setCursor]);

  return {
    state,
    setCursor,
    setSort,
    setFilter,
    setFilters,
    clearFilters,
    resetCursor,
  };
}
