import { useCallback, useState } from 'react';

interface CursorPage<T> {
  data: T[];
  page: { has_more: boolean };
}

/**
 * Drives cursor pagination over a TanStack `useInfiniteQuery` result. Tracks
 * the visible page index, exposes the current page's rows, and fetches the
 * next cursor on demand — the shared pattern behind every paginated table.
 */
export function useInfinitePager<T>(
  pages: readonly CursorPage<T>[],
  fetchNextPage: () => Promise<unknown>,
  hasNextPage: boolean,
  isFetchingNextPage: boolean
) {
  const [pageIndex, setPageIndex] = useState(0);

  const currentPage = pages[pageIndex];
  const rows = currentPage?.data ?? [];
  const canPrev = pageIndex > 0;
  const canNext = Boolean(currentPage?.page.has_more);

  const handlePrev = useCallback(() => {
    setPageIndex((index) => Math.max(0, index - 1));
  }, []);

  const handleNext = useCallback(() => {
    if (isFetchingNextPage) return;
    if (pageIndex + 1 < pages.length) {
      setPageIndex(pageIndex + 1);
      return;
    }
    if (hasNextPage) {
      void fetchNextPage().then(() => setPageIndex((index) => index + 1));
    }
  }, [pageIndex, pages.length, hasNextPage, isFetchingNextPage, fetchNextPage]);

  const reset = useCallback(() => setPageIndex(0), []);

  return { pageIndex, rows, canPrev, canNext, handlePrev, handleNext, reset };
}
