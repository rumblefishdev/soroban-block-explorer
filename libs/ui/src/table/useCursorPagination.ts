import { useCallback } from 'react';

import { useTableUrlState } from './useTableUrlState.js';

export interface UseCursorPaginationResult {
  cursor: string | null;
  goNext: (cursor: string) => void;
  goPrev: (cursor: string) => void;
  reset: () => void;
}

export function useCursorPagination(): UseCursorPaginationResult {
  const { state, setCursor, resetCursor } = useTableUrlState();
  const goNext = useCallback(
    (cursor: string) => setCursor(cursor),
    [setCursor]
  );
  const goPrev = useCallback(
    (cursor: string) => setCursor(cursor),
    [setCursor]
  );
  return { cursor: state.cursor, goNext, goPrev, reset: resetCursor };
}
