import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button, Card } from '@mui/material';
import {
  EmptyState,
  PaginationControls,
  QueryErrorState,
  TableEmptyState,
  type TableEmptyKind,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

interface DataListCardProps<T> {
  filters?: ReactNode;
  columnCount: number;
  isLoading: boolean;
  isError: boolean;
  error?: unknown;
  onRetry?: () => void;
  rows: readonly T[];

  renderTable: (rows: readonly T[]) => ReactNode;

  hasActiveFilters?: boolean;
  emptyKind: TableEmptyKind;
  emptyNoun: string;
  onClearFilters?: () => void;
  paginationCaption?: string;
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  skeletonRows?: number;
}

export function DataListCard<T>({
  filters,
  columnCount,
  isLoading,
  isError,
  error,
  onRetry,
  rows,
  renderTable,
  hasActiveFilters = false,
  emptyKind,
  emptyNoun,
  onClearFilters,
  paginationCaption = 'Latest results',
  canPrev,
  canNext,
  onPrev,
  onNext,
  skeletonRows = 10,
}: DataListCardProps<T>) {
  let body: ReactNode;
  if (isLoading) {
    // No padding wrapper — the loaded table (`renderTable`) renders flush, so
    // wrapping the skeleton in `p:2` shifted the table ~16px down on load.
    // Match the loaded layout so there's no loading→loaded jump.
    body = <TableSkeleton rows={skeletonRows} columns={columnCount} />;
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={onRetry} py={8} />;
  } else if (rows.length === 0) {
    body = hasActiveFilters ? (
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
    ) : (
      <TableEmptyState kind={emptyKind} />
    );
  } else {
    body = renderTable(rows);
  }

  return (
    <Card>
      {filters}
      <Box>{body}</Box>
      <PaginationControls
        caption={paginationCaption}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={onPrev}
        onNext={onNext}
      />
    </Card>
  );
}
