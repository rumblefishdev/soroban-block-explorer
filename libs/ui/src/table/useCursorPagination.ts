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
  // `goNext` and `goPrev` are intentionally identical: the API's pagination
  // cursors are opaque and already direction-encoded by the server (a "next"
  // page returns the next cursor; a "prev" page returns the prev cursor).
  // Both client actions are the same "navigate to this cursor" operation —
  // the asymmetry lives in the cursor value, not in the call site. The two
  // names are kept as documentation at the call site (`goNext(next)` reads
  // better than `goTo(next)` in a paginator).
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
