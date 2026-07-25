import {
  usePageHandlers,
  type PageInfoLike,
} from '@rumblefish/soroban-block-explorer-ui';

/**
 * Collapses the byte-identical tail of every cursor-paginated call-site —
 * `const rows = data?.data ?? []` plus the `usePageHandlers(data?.page, …)`
 * wiring — into one call. Data/logic only: it returns values, never JSX, so
 * it cannot change any rendering. Callers keep their own query hook, their
 * own loading/error/empty branches, and their own `useCursorPagination` (with
 * whatever `state` / `setFilter` / `setSort` they need).
 *
 * Sites whose response nests the page under another key (e.g. ledger detail's
 * `data.transactions.page`) don't fit this shape and stay hand-wired.
 */
export function usePagedRows<T>(
  data: { data: T[]; page?: PageInfoLike } | undefined,
  goNext: (cursor: string) => void,
  goPrev: (cursor: string | null) => void
) {
  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );
  return { rows, canPrev, canNext, handlePrev, handleNext };
}
