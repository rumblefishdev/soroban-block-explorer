import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import type { SortDirection } from './ExplorerTable.js';

const CURSOR_PARAM = 'cursor';
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
}

export interface UseTableUrlStateResult {
  state: TableUrlState;
  setCursor: (cursor: string | null) => void;
  setSort: (sortBy: string, sortDir: SortDirection) => void;
  setFilter: (key: string, value: string | null) => void;
  resetCursor: () => void;
}

export function useTableUrlState(
  options: UseTableUrlStateOptions = {}
): UseTableUrlStateResult {
  const { defaultSortBy, defaultSortDir = 'desc', filterKeys = [] } = options;
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
      cursor: params.get(CURSOR_PARAM),
      sortBy: params.get(SORT_PARAM) ?? defaultSortBy ?? null,
      sortDir,
      filters,
    };
  }, [params, filterKeysKey, defaultSortBy, defaultSortDir]);

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
        if (cursor) next.set(CURSOR_PARAM, cursor);
        else next.delete(CURSOR_PARAM);
      });
    },
    [update]
  );

  const setSort = useCallback(
    (sortBy: string, sortDir: SortDirection) => {
      update((next) => {
        next.set(SORT_PARAM, sortBy);
        next.set(DIR_PARAM, sortDir);
        next.delete(CURSOR_PARAM);
      });
    },
    [update]
  );

  const setFilter = useCallback(
    (key: string, value: string | null) => {
      update((next) => {
        if (value) next.set(key, value);
        else next.delete(key);
        next.delete(CURSOR_PARAM);
      });
    },
    [update]
  );

  const resetCursor = useCallback(() => setCursor(null), [setCursor]);

  return { state, setCursor, setSort, setFilter, resetCursor };
}
