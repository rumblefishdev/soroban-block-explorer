import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button } from '@mui/material';
import {
  EmptyState,
  PaginationControls,
  QueryErrorState,
  TableEmptyState,
  type TableEmptyKind,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

/** The fields of the page's query DataList reads — a `useQuery` result fits. */
export interface DataListQuery {
  isLoading: boolean;
  /**
   * `true` while a page change or filter change is fetching new data.
   * `keepPreviousData` keeps the old rows + cursors around, but we replace the
   * body with the skeleton so the user sees the table is reloading. Not the
   * same as `isLoading` (first load, no data yet).
   */
  isPlaceholderData: boolean;
  isError: boolean;
  error: unknown;
  refetch: () => unknown;
}

/** The current page's rows and its pager — a `usePagedRows` result fits. */
export interface DataListPager<T> {
  rows: readonly T[];
  canPrev: boolean;
  canNext: boolean;
  handlePrev: () => void;
  handleNext: () => void;
}

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

/** Filtered lists name what they list, for "No {emptyNoun} match your filters". */
type FilterStateProps =
  | {
      hasActiveFilters?: undefined;
      emptyNoun?: undefined;
      onClearFilters?: undefined;
    }
  | {
      hasActiveFilters: boolean;
      emptyNoun: string;
      onClearFilters?: () => void;
    };

export type DataListProps<T> = EmptyStateProps &
  FilterStateProps & {
    filters?: ReactNode;
    query: DataListQuery;
    pager: DataListPager<T>;
    /** Vertical padding of the error state. */
    errorPy?: number;
    /**
     * Render the table. On first load and while reloading it is called with no
     * rows and `loading: true`: the REAL table in its `loading` mode — same
     * headers, same column layout, same row height — so the skeleton is the
     * same height as the populated table at every viewport (no jump).
     */
    renderTable: (rows: readonly T[], state: { loading: boolean }) => ReactNode;
    paginationCaption?: string;
  };

/**
 * A paginated list without a frame: filters, then exactly one of skeleton /
 * error / empty / table, then the pager. The caller owns the frame —
 * `DataListCard` puts it in a plain `Card` (list pages); detail sections put
 * it in their own `SectionCard`, titled `Card` or tab.
 */
export function DataList<T>({
  filters,
  query,
  pager,
  errorPy = 8,
  renderTable,
  hasActiveFilters = false,
  emptyKind,
  emptyPy,
  renderEmpty,
  emptyNoun,
  onClearFilters,
  paginationCaption = 'Latest results',
}: DataListProps<T>) {
  let body: ReactNode;
  if (query.isLoading || query.isPlaceholderData) {
    // Skeleton on first load AND while a page/filter change is fetching, so
    // the user sees the table is reloading rather than the old rows sitting
    // silently.
    body = renderTable([], { loading: true });
  } else if (query.isError) {
    body = (
      <QueryErrorState
        error={query.error}
        onRetry={() => void query.refetch()}
        py={errorPy}
      />
    );
  } else if (pager.rows.length === 0) {
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
    body = renderTable(pager.rows, { loading: false });
  }

  return (
    <>
      {filters}
      <Box>{body}</Box>
      <PaginationControls
        caption={paginationCaption}
        canPrev={pager.canPrev}
        canNext={pager.canNext}
        onPrev={pager.handlePrev}
        onNext={pager.handleNext}
      />
    </>
  );
}
