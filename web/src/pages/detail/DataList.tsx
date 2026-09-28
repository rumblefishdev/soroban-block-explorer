import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button } from '@mui/material';
import {
  EmptyState,
  PaginationControls,
  QueryErrorState,
  TableEmptyState,
  type TableEmptyKind,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

/**
 * The unfiltered empty state — exactly one of: the standard `TableEmptyState`
 * preset (`emptyKind`, optionally with its padding `emptyPy`) or a
 * caller-rendered one (`renderEmpty`, sections with their own copy or icon).
 */
type EmptyStateProps =
  | { emptyKind: TableEmptyKind; emptyPy?: number; renderEmpty?: undefined }
  | {
      emptyKind?: undefined;
      emptyPy?: undefined;
      renderEmpty: () => ReactNode;
    };

export type DataListProps<T> = EmptyStateProps & {
  filters?: ReactNode;
  columnCount: number;
  isLoading: boolean;
  /**
   * `true` while a page change or filter change is fetching new data (React
   * Query `isPlaceholderData`). `keepPreviousData` keeps the old rows + cursors
   * around, but we replace the body with the skeleton so the user sees the
   * table is reloading. Not the same as `isLoading` (first load, no data yet).
   */
  isReloading?: boolean;
  isError: boolean;
  error?: unknown;
  onRetry?: () => void;
  /** Vertical padding of the error state. */
  errorPy?: number;
  rows: readonly T[];

  renderTable: (rows: readonly T[]) => ReactNode;

  /**
   * Render the loading skeleton using the REAL table in its `loading` mode
   * (`<XxxTable loading />`) — same headers, same column layout, same row
   * height — so the skeleton is the same height as the populated table at every
   * viewport (responsive, no jump). Falls back to the generic `TableSkeleton`
   * when not provided.
   */
  renderSkeleton?: () => ReactNode;

  hasActiveFilters?: boolean;
  emptyNoun: string;
  onClearFilters?: () => void;
  paginationCaption?: string;
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  skeletonRows?: number;
};

/**
 * A paginated list without a frame: filters, then exactly one of skeleton /
 * error / empty / table, then the pager. The caller owns the frame —
 * `DataListCard` puts it in a plain `Card` (list pages); detail sections put
 * it in their own `SectionCard`, titled `Card` or tab.
 */
export function DataList<T>({
  filters,
  columnCount,
  isLoading,
  isReloading = false,
  isError,
  error,
  onRetry,
  errorPy = 8,
  rows,
  renderTable,
  renderSkeleton,
  hasActiveFilters = false,
  emptyKind,
  emptyPy,
  renderEmpty,
  emptyNoun,
  onClearFilters,
  paginationCaption = 'Latest results',
  canPrev,
  canNext,
  onPrev,
  onNext,
  // Default to a full list page (all list pages use `PAGE_SIZE = 20`) so the
  // skeleton matches the populated table's height — no jump on the data ↔
  // skeleton swap during pagination / filter changes. A page with a different
  // size passes `skeletonRows` explicitly.
  skeletonRows = 20,
}: DataListProps<T>) {
  let body: ReactNode;
  if (isLoading || isReloading) {
    // Skeleton on first load (`isLoading`) AND while a page/filter change is
    // fetching (`isReloading` = `isPlaceholderData`), so the user sees the
    // table is reloading rather than the old rows sitting silently.
    // Prefer `renderSkeleton` (the real table in `loading` mode) — it matches
    // the populated table's height at every viewport. The generic
    // `TableSkeleton` fallback is height-matched on wide screens but can drift
    // when headers wrap on narrow ones.
    body = renderSkeleton ? (
      renderSkeleton()
    ) : (
      <TableSkeleton rows={skeletonRows} columns={columnCount} />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={onRetry} py={errorPy} />;
  } else if (rows.length === 0) {
    if (hasActiveFilters) {
      body = (
        <EmptyState
          icon={<SearchIcon />}
          title={`No ${emptyNoun} match your filters`}
          description="Try adjusting or clearing the active filters"
          action={
            onClearFilters ? (
              <Button variant="contained" onClick={onClearFilters}>
                Clear filters
              </Button>
            ) : undefined
          }
          py={8}
        />
      );
    } else if (renderEmpty) {
      body = renderEmpty();
    } else {
      body = <TableEmptyState kind={emptyKind} py={emptyPy} />;
    }
  } else {
    body = renderTable(rows);
  }

  return (
    <>
      {filters}
      <Box>{body}</Box>
      <PaginationControls
        caption={paginationCaption}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={onPrev}
        onNext={onNext}
      />
    </>
  );
}
